//! Romaji → hiragana conversion driven by Mozc's table.

use std::collections::{HashMap, HashSet};

pub struct Romaji {
    map: HashMap<String, (String, String)>,
    /// strings that are a *proper* prefix of some key
    prefixes: HashSet<String>,
}

impl Default for Romaji {
    fn default() -> Self {
        Self::new()
    }
}

impl Romaji {
    pub fn new() -> Self {
        let mut map = HashMap::new();
        let mut prefixes = HashSet::new();
        for line in include_str!("../romaji.tsv").lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 2 || f[0].is_empty() {
                continue;
            }
            let pend = f.get(2).copied().unwrap_or("").to_string();
            map.insert(f[0].to_string(), (f[1].to_string(), pend));
            for i in 1..f[0].len() {
                if f[0].is_char_boundary(i) {
                    prefixes.insert(f[0][..i].to_string());
                }
            }
        }
        Romaji { map, prefixes }
    }

    /// Convert keystrokes. Returns (converted, pending): `pending` is trailing
    /// romaji that may still become kana when more keys arrive. With `flush`,
    /// a trailing "n" becomes ん.
    pub fn convert(&self, input: &str, flush: bool) -> (String, String) {
        let (chunks, pending) = self.convert_aligned(input, flush);
        (chunks.into_iter().map(|c| c.out).collect(), pending)
    }

    /// Like `convert`, but reports which raw (char) range produced each piece of output.
    pub fn convert_aligned(&self, input: &str, flush: bool) -> (Vec<Chunk>, String) {
        let mut chunks: Vec<Chunk> = Vec::new();
        let mut pending = String::new();
        let mut pstart = 0usize;
        for c in input.chars() {
            pending.push(c.to_ascii_lowercase());
            self.resolve(&mut pending, &mut pstart, &mut chunks, false);
        }
        if flush {
            self.resolve(&mut pending, &mut pstart, &mut chunks, true);
        }
        (chunks, pending)
    }

    /// Like `convert_aligned(.., true)` but flushes pending romaji at each raw char index in `breaks`.
    pub fn convert_aligned_breaks(&self, input: &str, breaks: &[usize]) -> (Vec<Chunk>, String) {
        let mut chunks: Vec<Chunk> = Vec::new();
        let mut pending = String::new();
        let mut pstart = 0usize;
        for (i, c) in input.chars().enumerate() {
            if breaks.contains(&i) {
                self.resolve(&mut pending, &mut pstart, &mut chunks, true);
            }
            pending.push(c.to_ascii_lowercase());
            self.resolve(&mut pending, &mut pstart, &mut chunks, false);
        }
        self.resolve(&mut pending, &mut pstart, &mut chunks, true);
        (chunks, pending)
    }

    fn resolve(&self, pending: &mut String, pstart: &mut usize, out: &mut Vec<Chunk>, flush: bool) {
        let emit = |out: &mut Vec<Chunk>, s: &str, start: usize, consumed: usize| {
            out.push(Chunk { out: s.to_string(), start, end: start + consumed });
        };
        loop {
            if pending.is_empty() {
                break;
            }
            let is_prefix = self.prefixes.contains(pending.as_str());
            if let Some((o, p)) = self.map.get(pending.as_str()) {
                if !is_prefix || flush {
                    let consumed = pending.chars().count() - p.chars().count();
                    emit(out, o, *pstart, consumed);
                    *pstart += consumed;
                    *pending = p.clone();
                    continue;
                }
                break;
            }
            if is_prefix {
                break;
            }
            // dead end: longest exact prefix wins, else emit one raw char
            let mut done = false;
            let idxs: Vec<usize> = pending.char_indices().map(|(i, _)| i).skip(1).collect();
            for &l in idxs.iter().rev() {
                if let Some((o, p)) = self.map.get(&pending[..l]) {
                    let consumed = pending[..l].chars().count() - p.chars().count();
                    emit(out, o, *pstart, consumed);
                    *pstart += consumed;
                    let rest = format!("{}{}", p, &pending[l..]);
                    *pending = rest;
                    done = true;
                    break;
                }
            }
            if !done {
                let c = pending.remove(0);
                emit(out, &c.to_string(), *pstart, 1);
                *pstart += 1;
            }
        }
        if flush && !pending.is_empty() {
            // unconvertible tail (e.g. a lone "k"): keep it as raw text
            let n = pending.chars().count();
            emit(out, pending, *pstart, n);
            *pstart += n;
            pending.clear();
        }
    }
}

/// A piece of kana produced from raw keystrokes `start..end` (char indices).
#[derive(Clone, Debug)]
pub struct Chunk {
    pub out: String,
    pub start: usize,
    pub end: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic() {
        let r = Romaji::new();
        assert_eq!(r.convert("ekusutensyon", true).0, "えくすてんしょん");
        assert_eq!(r.convert("kitte", true).0, "きって");
        assert_eq!(r.convert("konnnitiha", true).0, "こんにちは");
        assert_eq!(r.convert("kan", false), ("か".to_string(), "n".to_string()));
        assert_eq!(r.convert("kan", true).0, "かん");
        assert_eq!(r.convert("kafe", true).0, "かふぇ");
    }
}
