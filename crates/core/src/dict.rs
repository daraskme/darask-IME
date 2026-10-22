//! Read-only access to the data produced by darask-build.

use fst::raw::Output;
use fst::Map;
use std::fs;
use std::io;
use std::path::Path;

pub struct Dict {
    reading: Map<Vec<u8>>,
    entries: Vec<u8>,
    surfaces: Vec<u8>,
    conn: Vec<i16>,
    conn_n: usize,
    pos_class: Vec<u8>,
    en: Map<Vec<u8>>,
    en_pool: String,
    enfreq: Map<Vec<u8>>,
    pub noun_general: u16,
    pub noun_proper: u16,
    pub symbol_general: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct Entry<'a> {
    pub lid: u16,
    pub rid: u16,
    pub cost: u16,
    pub surface: &'a str,
}

impl Dict {
    pub fn open(dir: &Path) -> io::Result<Dict> {
        let rd = |n: &str| fs::read(dir.join(n));
        let mk = |b: Vec<u8>| Map::new(b).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
        let conn_raw = rd("conn.bin")?;
        let conn_n = u32::from_le_bytes(conn_raw[..4].try_into().unwrap()) as usize;
        let conn: Vec<i16> = conn_raw[4..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        let meta = fs::read_to_string(dir.join("meta.txt"))?;
        let get = |k: &str| -> u16 {
            meta.lines()
                .find_map(|l| l.strip_prefix(&format!("{}=", k)))
                .and_then(|v| v.parse().ok())
                .unwrap_or(1851)
        };
        Ok(Dict {
            reading: mk(rd("dict.fst")?)?,
            entries: rd("entries.bin")?,
            surfaces: rd("surfaces.bin")?,
            conn,
            conn_n,
            pos_class: rd("pos.bin")?,
            en: mk(rd("en.fst")?)?,
            en_pool: fs::read_to_string(dir.join("en_pool.txt"))?,
            enfreq: mk(rd("enfreq.fst")?)?,
            noun_general: get("noun_general"),
            noun_proper: get("noun_proper"),
            symbol_general: get("symbol_general"),
        })
    }

    fn entry(&self, i: usize) -> Entry<'_> {
        let b = &self.entries[i * 12..i * 12 + 12];
        let soff = u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize;
        let slen = u16::from_le_bytes([b[6], b[7]]) as usize;
        Entry {
            lid: u16::from_le_bytes([b[0], b[1]]),
            rid: u16::from_le_bytes([b[2], b[3]]),
            cost: u16::from_le_bytes([b[4], b[5]]),
            surface: std::str::from_utf8(&self.surfaces[soff..soff + slen]).unwrap_or(""),
        }
    }

    /// Entries whose reading is exactly `reading`.
    pub fn lookup(&self, reading: &str) -> Vec<Entry<'_>> {
        match self.reading.get(reading) {
            Some(v) => self.range(v),
            None => Vec::new(),
        }
    }

    fn range(&self, v: u64) -> Vec<Entry<'_>> {
        let start = (v >> 16) as usize;
        let count = (v & 0xffff) as usize;
        (start..start + count).map(|i| self.entry(i)).collect()
    }

    /// Common-prefix search: every dictionary reading that is a prefix of `s`.
    /// Yields (byte_len_of_reading, entries).
    pub fn prefixes<'a>(&'a self, s: &str) -> Vec<(usize, Vec<Entry<'a>>)> {
        let fst = self.reading.as_fst();
        let mut node = fst.root();
        let mut out = Output::zero();
        let mut res = Vec::new();
        for (i, &b) in s.as_bytes().iter().enumerate() {
            let Some(ti) = node.find_input(b) else { break };
            let t = node.transition(ti);
            out = out.cat(t.out);
            node = fst.node(t.addr);
            if node.is_final() {
                res.push((i + 1, self.range(out.cat(node.final_output()).value())));
            }
        }
        res
    }

    #[inline]
    pub fn conn(&self, rid_prev: u16, lid_next: u16) -> i32 {
        let (a, b) = (rid_prev as usize, lid_next as usize);
        if a < self.conn_n && b < self.conn_n {
            self.conn[a * self.conn_n + b] as i32
        } else {
            10000
        }
    }

    pub fn pos_class(&self, id: u16) -> u8 {
        self.pos_class.get(id as usize).copied().unwrap_or(0)
    }

    /// English words whose katakana rendering is `kata`, best first.
    pub fn english_for(&self, kata: &str) -> Vec<&str> {
        let key = kata.replace('ー', "");
        match self.en.get(key.as_str()) {
            Some(off) => self.en_pool[off as usize..]
                .lines()
                .next()
                .unwrap_or("")
                .split('\t')
                .collect(),
            None => Vec::new(),
        }
    }

    /// Frequency rank of an English word (0 = most common), if known.
    pub fn english_rank(&self, w: &str) -> Option<u64> {
        self.enfreq.get(w)
    }
}

impl Dict {
    /// Dictionary entries whose reading starts with `prefix` (excluding exact), at most `limit` readings scanned.
    pub fn predict(&self, prefix: &str, limit: usize) -> Vec<(String, Vec<Entry<'_>>)> {
        use fst::automaton::{Automaton, Str};
        use fst::{IntoStreamer, Streamer};
        let aut = Str::new(prefix).starts_with();
        let mut st = self.reading.search(aut).into_stream();
        let mut out = Vec::new();
        while let Some((k, v)) = st.next() {
            if out.len() >= limit {
                break;
            }
            let k = String::from_utf8_lossy(k).to_string();
            if k == prefix {
                continue;
            }
            out.push((k, self.range(v)));
        }
        out
    }

    /// English words whose katakana rendering starts with `kata_prefix`.
    pub fn english_predict(&self, kata_prefix: &str, limit: usize) -> Vec<(String, &str)> {
        let kata_prefix = kata_prefix.replace('ー', "");
        let kata_prefix = kata_prefix.as_str();
        use fst::automaton::{Automaton, Str};
        use fst::{IntoStreamer, Streamer};
        let aut = Str::new(kata_prefix).starts_with();
        let mut st = self.en.search(aut).into_stream();
        let mut out = Vec::new();
        while let Some((k, v)) = st.next() {
            if out.len() >= limit {
                break;
            }
            let word = self.en_pool[v as usize..].lines().next().unwrap_or("").split('\t').next().unwrap_or("");
            out.push((String::from_utf8_lossy(k).to_string(), word));
        }
        out
    }
}
