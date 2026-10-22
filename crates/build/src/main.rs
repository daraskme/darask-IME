//! Builds the runtime data files for darask-IME.
//!
//!   darask-build <mozc-src-dir> <data/src dir> <out dir>
//!
//! Output files (all read by darask-core):
//!   dict.fst      reading -> (entry_start << 16 | count)
//!   entries.bin   12-byte records: lid u16, rid u16, cost u16, slen u16, soff u32
//!   surfaces.bin  UTF-8 surface pool
//!   conn.bin      u32 N, then N*N i16 connection costs  [rid_prev * N + lid_next]
//!   meta.txt      key=value (POS ids)
//!   en.fst        katakana reading -> offset into en_pool.txt
//!   en_pool.txt   one line per reading: "word\tword\t..." (best first)
//!   enfreq.fst    english word -> rank (0 = most frequent)

mod phon;

use fst::{Map, MapBuilder};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: darask-build <mozc-src> <data/src> <out>");
        std::process::exit(2);
    }
    let mozc = Path::new(&args[1]).join("src/data");
    let src = Path::new(&args[2]);
    let out = Path::new(&args[3]);
    fs::create_dir_all(out).unwrap();

    build_dict(&mozc, out);
    build_conn(&mozc, out);
    build_english(src, out);
}

fn build_dict(mozc: &Path, out: &Path) {
    let dir = mozc.join("dictionary_oss");
    let mut rows: Vec<(String, u16, u16, u16, String)> = Vec::new();
    for i in 0..10 {
        let text = fs::read_to_string(dir.join(format!("dictionary{:02}.txt", i))).unwrap();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 5 {
                continue;
            }
            let cost: i32 = f[3].parse().unwrap_or(10000);
            rows.push((
                f[0].to_string(),
                f[1].parse().unwrap(),
                f[2].parse().unwrap(),
                cost.clamp(0, 65535) as u16,
                f[4].to_string(),
            ));
        }
    }
    // long-vowel tolerance: also index readings without ー (ストレージ ← すとれじ), slightly penalised
    let extra: Vec<_> = rows
        .iter()
        .filter(|r| r.0.contains('ー') && r.0.chars().any(|c| c != 'ー'))
        .map(|r| (r.0.replace('ー', ""), r.1, r.2, r.3.saturating_add(900), r.4.clone()))
        .collect();
    rows.extend(extra);
    // the same file also carries a few manual additions
    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.3.cmp(&b.3)));
    rows.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2 && a.4 == b.4);

    let mut entries = BufWriter::new(File::create(out.join("entries.bin")).unwrap());
    let mut surfaces: Vec<u8> = Vec::new();
    let mut fstb = MapBuilder::new(BufWriter::new(File::create(out.join("dict.fst")).unwrap())).unwrap();

    let mut i = 0usize;
    while i < rows.len() {
        let mut j = i;
        while j < rows.len() && rows[j].0 == rows[i].0 {
            j += 1;
        }
        let count = (j - i).min(65535);
        fstb.insert(rows[i].0.as_bytes(), ((i as u64) << 16) | count as u64).unwrap();
        i = j;
    }
    fstb.finish().unwrap();
    for r in &rows {
        let soff = surfaces.len() as u32;
        surfaces.extend_from_slice(r.4.as_bytes());
        entries.write_all(&r.1.to_le_bytes()).unwrap();
        entries.write_all(&r.2.to_le_bytes()).unwrap();
        entries.write_all(&r.3.to_le_bytes()).unwrap();
        entries.write_all(&(r.4.len() as u16).to_le_bytes()).unwrap();
        entries.write_all(&soff.to_le_bytes()).unwrap();
    }
    entries.flush().unwrap();
    fs::write(out.join("surfaces.bin"), surfaces).unwrap();
    eprintln!("dict: {} entries", rows.len());

    // POS ids we need for synthetic nodes
    let ids = fs::read_to_string(dir.join("id.def")).unwrap();
    let mut meta = String::new();
    let want = [
        ("noun_general", "名詞,一般,*,*,*,*,*"),
        ("noun_proper", "名詞,固有名詞,一般,*,*,*,*"),
        ("symbol_general", "記号,一般,*,*,*,*,*"),
        ("noun_suffix", "名詞,接尾,一般,*,*,*,*"),
    ];
    for line in ids.lines() {
        let (id, rest) = line.split_once(' ').unwrap_or(("", ""));
        for (k, v) in want {
            if rest == v {
                meta.push_str(&format!("{}={}\n", k, id));
            }
        }
    }
    fs::write(out.join("meta.txt"), meta).unwrap();

    // coarse POS class per id: 0 content, 1 functional (sticks to previous), 2 prefix (sticks to next)
    let mut cls = vec![0u8; 4096];
    for line in ids.lines() {
        let (id, rest) = line.split_once(' ').unwrap_or(("", ""));
        let Ok(id) = id.parse::<usize>() else { continue };
        let functional = ["助詞", "助動詞", "名詞,接尾", "動詞,接尾", "動詞,非自立", "形容詞,非自立", "名詞,非自立", "形容詞,接尾"];
        cls[id] = if functional.iter().any(|p| rest.starts_with(p)) { 1 } else if rest.starts_with("接頭詞") { 2 } else { 0 };
    }
    fs::write(out.join("pos.bin"), cls).unwrap();
}

fn build_conn(mozc: &Path, out: &Path) {
    let text = fs::read_to_string(mozc.join("dictionary_oss/connection_single_column.txt")).unwrap();
    let mut it = text.lines();
    let n: u32 = it.next().unwrap().trim().parse().unwrap();
    let mut w = BufWriter::new(File::create(out.join("conn.bin")).unwrap());
    w.write_all(&n.to_le_bytes()).unwrap();
    let mut cnt = 0u64;
    for l in it {
        let v: i32 = l.trim().parse().unwrap();
        w.write_all(&(v.clamp(-32768, 32767) as i16).to_le_bytes()).unwrap();
        cnt += 1;
    }
    assert_eq!(cnt, (n as u64) * (n as u64));
    w.flush().unwrap();
    eprintln!("conn: {}x{}", n, n);
}

fn build_english(src: &Path, out: &Path) {
    // 1. frequency ranks
    let mut freq: HashMap<String, usize> = HashMap::new();
    let text = fs::read_to_string(src.join("count_1w.txt")).unwrap();
    for (rank, line) in text.lines().enumerate() {
        if let Some((w, _)) = line.split_once('\t') {
            freq.insert(w.to_string(), rank);
        }
    }
    // 2. CMUdict pronunciations -> katakana
    let cmu = fs::read_to_string(src.join("cmudict.dict")).unwrap();
    // reading -> (word, rank, variant_penalty)
    let mut by_kana: HashMap<String, Vec<(String, usize, usize)>> = HashMap::new();
    for line in cmu.lines() {
        let mut parts = line.split_whitespace();
        let Some(word) = parts.next() else { continue };
        let word = word.split('(').next().unwrap();
        if !word.bytes().all(|b| b.is_ascii_lowercase()) || word.len() < 2 {
            continue;
        }
        let Some(&rank) = freq.get(word) else { continue };
        if rank > 120_000 {
            continue;
        }
        let phones: Vec<&str> = parts.collect();
        for (vi, k) in phon::to_katakana(&phones).into_iter().enumerate() {
            by_kana.entry(k).or_default().push((word.to_string(), rank, vi));
        }
    }
    // 3. JMdict loanword sources are authoritative: they go first
    let mut jm: HashMap<String, Vec<String>> = HashMap::new();
    if let Ok(xml) = fs::read_to_string(src.join("JMdict_e")) {
        parse_jmdict_loans(&xml, &mut jm);
    }
    for (k, ws) in &jm {
        for w in ws {
            let rank = freq.get(w.as_str()).copied().unwrap_or(150_000);
            by_kana.entry(k.clone()).or_default().push((w.clone(), rank.saturating_sub(5000), 0));
        }
    }

    for (k, ws) in load_extra(src) {
        for (i, w) in ws.into_iter().enumerate() {
            by_kana.entry(k.clone()).or_default().push((w, 0, i));
        }
    }

    // 4. emit (keys ignore the long-vowel mark so コンピュータ == コンピューター)
    let mut merged: HashMap<String, Vec<(String, usize, usize)>> = HashMap::new();
    for (k, v) in by_kana {
        merged.entry(k.replace('ー', "")).or_default().extend(v);
    }
    let mut sorted: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (k, mut v) in merged {
        v.sort_by_key(|x| (x.1 + x.2 * 3000, x.0.clone()));
        let mut seen = std::collections::HashSet::new();
        v.retain(|x| seen.insert(x.0.clone()));
        sorted.insert(k, v.into_iter().take(6).map(|x| x.0).collect());
    }
    let mut pool = String::new();
    let mut b = MapBuilder::new(BufWriter::new(File::create(out.join("en.fst")).unwrap())).unwrap();
    for (k, ws) in &sorted {
        let off = pool.len() as u64;
        pool.push_str(&ws.join("\t"));
        pool.push('\n');
        b.insert(k.as_bytes(), off).unwrap();
    }
    b.finish().unwrap();
    fs::write(out.join("en_pool.txt"), pool).unwrap();
    eprintln!("english: {} katakana keys", sorted.len());

    let mut words: Vec<(&String, &usize)> = freq.iter().filter(|(_, r)| **r < 200_000).collect();
    words.sort_by(|a, b| a.0.cmp(b.0));
    let mut b = MapBuilder::new(BufWriter::new(File::create(out.join("enfreq.fst")).unwrap())).unwrap();
    for (w, r) in words {
        b.insert(w.as_bytes(), *r as u64).unwrap();
    }
    b.finish().unwrap();
    let _: Option<Map<Vec<u8>>> = None;
}

/// Extract katakana reading -> english lsource words from JMdict.
fn parse_jmdict_loans(xml: &str, out: &mut HashMap<String, Vec<String>>) {
    for entry in xml.split("<entry>").skip(1) {
        let rebs: Vec<&str> = entry
            .split("<reb>")
            .skip(1)
            .filter_map(|s| s.split("</reb>").next())
            .collect();
        let srcs: Vec<String> = entry
            .split("<lsource xml:lang=\"eng\">")
            .skip(1)
            .filter_map(|s| s.split("</lsource>").next())
            .map(|s| s.to_string())
            .collect();
        for s in srcs {
            // keep only plain single lowercase words ("accel(erator)" etc. are skipped)
            if !s.bytes().all(|b| b.is_ascii_alphabetic()) || s.len() < 2 {
                continue;
            }
            let w = s.to_lowercase();
            for r in &rebs {
                if r.chars().all(|c| ('ァ'..='ー').contains(&c)) {
                    out.entry(r.to_string()).or_default().push(w.clone());
                }
            }
        }
    }
}

/// Hand-curated katakana → English entries (data/src/extra_en.tsv). They outrank generated ones.
fn load_extra(src: &Path) -> Vec<(String, Vec<String>)> {
    let mut v = Vec::new();
    if let Ok(t) = fs::read_to_string(src.join("extra_en.tsv")) {
        for l in t.lines() {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() >= 2 {
                v.push((f[0].to_string(), f[1..].iter().map(|s| s.to_string()).collect()));
            }
        }
    }
    v
}
