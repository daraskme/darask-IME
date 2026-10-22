//! darask-core: conversion engine with candidate generation tuned for
//! loanwords, English and mixed-script writing.

pub mod dict;
pub mod history;
pub mod kana;
pub mod lattice;
pub mod romaji;
pub mod techwords;
pub mod session;

use dict::Dict;
use history::History;
use kana::{foreign_score, hira_to_kata, to_fullwidth_ascii, to_halfwidth_kana};
use lattice::{BuildOpts, Lattice};
use std::collections::HashMap;
use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// from the dictionary / lattice
    Conversion,
    Hiragana,
    Katakana,
    /// katakana only for part of the reading
    PartialKatakana,
    English,
    /// what the user literally typed
    Raw,
    HalfKatakana,
    FullAlpha,
    Predict,
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub text: String,
    pub kind: Kind,
    /// lower is better
    pub score: i32,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub reading: String,
    pub candidates: Vec<Candidate>,
}

pub struct Engine {
    pub dict: Dict,
    pub romaji: romaji::Romaji,
    pub history: History,
}

const NBEST: usize = 12;

impl Engine {
    pub fn open(data_dir: &Path, history_path: Option<&Path>) -> io::Result<Engine> {
        Ok(Engine {
            dict: Dict::open(data_dir)?,
            romaji: romaji::Romaji::new(),
            history: match history_path {
                Some(p) => History::load(p),
                None => History::in_memory(),
            },
        })
    }

    /// Hiragana preedit for the typed keys, plus un-resolved trailing romaji.
    pub fn preedit(&self, raw: &str) -> (String, String) {
        self.romaji.convert(raw, false)
    }

    /// Convert typed keys (romaji or kana) into segments with ranked candidates.
    pub fn convert(&self, raw: &str) -> Vec<Segment> {
        self.convert_spans(raw, None).0
    }

    /// English words that can be read off the raw keystrokes at positions that line up
    /// with kana boundaries (so "gitにpush" style mixed input can be segmented).
    fn english_nodes(&self, raw: &str, chunks: &[romaji::Chunk]) -> Vec<lattice::Node> {
        let rawc: Vec<char> = raw.chars().collect();
        // raw boundary -> kana char index
        let mut bound: Vec<(usize, usize)> = Vec::new();
        let mut kidx = 0usize;
        for c in chunks {
            // only chunks emitting a single kana char (or ASCII passthrough) give clean boundaries
            bound.push((c.start, kidx));
            kidx += c.out.chars().count();
        }
        if let Some(last) = chunks.last() {
            bound.push((last.end, kidx));
        }
        let mut out = Vec::new();
        for (bi, &(rs, ks)) in bound.iter().enumerate() {
            if !rawc.get(rs).map_or(false, |c| c.is_ascii_alphabetic()) {
                continue;
            }
            for &(re, ke) in bound.iter().skip(bi + 1) {
                if re - rs > 24 {
                    break;
                }
                if re - rs < 2 || !rawc[rs..re].iter().all(|c| c.is_ascii_alphabetic()) {
                    continue;
                }
                let word: String = rawc[rs..re].iter().collect();
                let lower = word.to_ascii_lowercase();
                let upper_start = rawc[rs].is_ascii_uppercase();
                let cost = if techwords::contains(&lower) {
                    4200
                } else if re - rs >= 3 {
                    match self.dict.english_rank(&lower) {
                        Some(r) if r < 50000 => 5200 + (r / 12).min(3000) as i32,
                        _ => continue,
                    }
                } else {
                    continue;
                };
                let cost = if upper_start { cost - 1800 } else { cost };
                out.push(lattice::Node {
                    start: ks,
                    end: ke,
                    lid: self.dict.noun_general,
                    rid: self.dict.noun_general,
                    wcost: cost,
                    surface: word,
                    synthetic: true,
                });
            }
        }
        out
    }

    fn user_nodes(&self, reading: &str) -> Vec<(String, i32)> {
        self.history
            .by_reading(reading)
            .into_iter()
            .map(|(s, c)| (s, (3500 - (1400.0 * (1.0 + c as f64).ln()) as i32).max(300)))
            .collect()
    }

    /// Convert with optional user-adjusted segment spans (char ranges over the kana).
    /// Returns the segments and the spans actually used.
    pub fn convert_spans(&self, raw: &str, forced: Option<&[(usize, usize)]>) -> (Vec<Segment>, Vec<(usize, usize)>) {
        // Candidate readings of the keystrokes: the plain romaji parse, plus parses that force
        // an English word's edges to be kana boundaries (so "pushisita" can be push + した).
        let (base_chunks, _) = self.romaji.convert_aligned(raw, true);
        let mut plans: Vec<(i32, Vec<romaji::Chunk>)> = vec![(0, base_chunks.clone())];
        for (i, j, bonus) in self.unaligned_english(raw, &base_chunks).into_iter().take(4) {
            let (c, _) = self.romaji.convert_aligned_breaks(raw, &[i, j]);
            plans.push((bonus, c));
        }
        let mut chosen: Option<(i32, Vec<romaji::Chunk>)> = None;
        for (plan_bonus, chunks) in plans {
            let kana: String = chunks.iter().map(|c| c.out.as_str()).collect();
            if kana.is_empty() {
                continue;
            }
            let chars: Vec<char> = kana.chars().collect();
            let extra = self.english_nodes(raw, &chunks);
            let bonus = |r: &str, s: &str| self.history.bonus(r, s);
            let user = |r: &str| self.user_nodes(r);
            let opts = BuildOpts { bonus: &bonus, user: &user, extra: &extra };
            let lat = Lattice::build(&self.dict, &chars, 0, chars.len(), 0, 0, &opts);
            let cost = lat.nbest(&self.dict, 1).first().map_or(i32::MAX, |p| p.cost.saturating_sub(plan_bonus));
            if std::env::var("DARASK_TRACE").is_ok() {
                eprintln!("plan {} cost {}", kana, cost);
            }
            if chosen.as_ref().map_or(true, |(c, _)| cost < *c) {
                chosen = Some((cost, chunks));
            }
        }
        let Some((_, chunks)) = chosen else {
            return (Vec::new(), Vec::new());
        };
        let kana: String = chunks.iter().map(|c| c.out.as_str()).collect();
        if kana.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let chars: Vec<char> = kana.chars().collect();
        let extra = self.english_nodes(raw, &chunks);
        let bonus = |r: &str, s: &str| self.history.bonus(r, s);
        let user = |r: &str| self.user_nodes(r);
        let opts = BuildOpts { bonus: &bonus, user: &user, extra: &extra };
        let full = Lattice::build(&self.dict, &chars, 0, chars.len(), 0, 0, &opts);
        let best = full.nbest(&self.dict, 1);
        let Some(best) = best.first() else {
            return (vec![self.fallback_segment(&kana)], vec![(0, chars.len())]);
        };

        // Whole input is one English word → literal word first.
        if forced.is_none() && best.nodes.len() == 1 {
            let n = &full.nodes[best.nodes[0]];
            if n.surface.bytes().all(|b| b.is_ascii_alphabetic()) {
                if let Some(seg) = self.english_whole(raw, &kana) {
                    return (vec![seg], vec![(0, chars.len())]);
                }
            }
        }

        // segmentation of the best path
        let spans: Vec<(usize, usize, u16, u16)> = match forced {
            Some(f) => f.iter().map(|&(s, e)| (s, e, 0u16, 0u16)).collect(),
            None => {
                let mut spans: Vec<(usize, usize, u16, u16)> = Vec::new(); // start,end,first_lid,last_rid
                for (k, &id) in best.nodes.iter().enumerate() {
                    let n = &full.nodes[id];
                    let join = k > 0 && {
                        let prev = &full.nodes[best.nodes[k - 1]];
                        let cls = self.dict.pos_class(n.lid);
                        let prev_cls = self.dict.pos_class(prev.rid);
                        cls == 1
                            || prev_cls == 2
                            || (n.synthetic && n.end - n.start == 1 && kana::is_hiragana(chars[n.start]) && prev.synthetic)
                    };
                    if join {
                        let last = spans.last_mut().unwrap();
                        last.1 = n.end;
                        last.3 = n.rid;
                    } else {
                        spans.push((n.start, n.end, n.lid, n.rid));
                    }
                }
                spans
            }
        };

        let mut segs = Vec::new();
        for (i, &(s, e, _, _)) in spans.iter().enumerate() {
            let left_rid = if i == 0 { 0 } else { spans[i - 1].3 };
            let right_lid = if i + 1 == spans.len() { 0 } else { spans[i + 1].2 };
            let reading: String = chars[s..e].iter().collect();
            let raw_for_seg = if spans.len() == 1 { Some(raw) } else { None };
            segs.push(self.segment_candidates(&chars, s, e, left_rid, right_lid, &reading, raw_for_seg, &opts));
        }
        (segs, spans.iter().map(|s| (s.0, s.1)).collect())
    }

    fn fallback_segment(&self, kana: &str) -> Segment {
        Segment {
            reading: kana.to_string(),
            candidates: vec![Candidate { text: kana.to_string(), kind: Kind::Hiragana, score: 0 }],
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn segment_candidates(
        &self,
        chars: &[char],
        s: usize,
        e: usize,
        left_rid: u16,
        right_lid: u16,
        reading: &str,
        raw: Option<&str>,
        opts: &BuildOpts,
    ) -> Segment {
        let lat = Lattice::build(&self.dict, chars, s, e, left_rid, right_lid, opts);
        let paths = lat.nbest(&self.dict, NBEST);
        let base = paths.first().map(|p| p.cost).unwrap_or(0);
        let mut cands: HashMap<String, Candidate> = HashMap::new();
        let put = |cands: &mut HashMap<String, Candidate>, text: String, kind: Kind, score: i32| {
            cands
                .entry(text.clone())
                .and_modify(|c| {
                    if score < c.score {
                        c.score = score;
                        c.kind = kind;
                    }
                })
                .or_insert(Candidate { text, kind, score });
        };

        for p in &paths {
            let text: String = p.nodes.iter().map(|&i| lat.nodes[i].surface.as_str()).collect();
            let all_kata_synth = p.nodes.iter().any(|&i| lat.nodes[i].synthetic && kana::is_katakana(lat.nodes[i].surface.chars().next().unwrap_or('a')));
            let kind = if all_kata_synth && p.nodes.len() > 1 { Kind::PartialKatakana } else { Kind::Conversion };
            put(&mut cands, text.clone(), kind, p.cost - base);
        }
        let kata = hira_to_kata(reading);
        let hira = kana::kata_to_hira(reading);
        let fs = foreign_score(reading) as i32;

        put(&mut cands, hira.clone(), Kind::Hiragana, if fs > 0 { 5000 } else { 2600 });
        // The lattice already has katakana nodes, but make sure the pure form is always there.
        put(&mut cands, kata.clone(), Kind::Katakana, if fs > 0 { 300 } else { 3600 });
        if kana::is_kana_str(reading) {
            // explicit partial-katakana variants for readings with a loanword-only chunk
            // (e.g. きふぇをのむ → キフェをのむ) so they are reachable even outside the lattice
            put(&mut cands, to_halfwidth_kana(&kata), Kind::HalfKatakana, 12000);
        }

        // ---- English via katakana reading -------------------------------------------------
        let kata_score = cands.get(&kata).map(|c| c.score).unwrap_or(3600);
        // English follows the katakana form only when that form is itself a leading candidate;
        // for ordinary Japanese words (のむ) English noise stays near the bottom.
        let kata_score = if kata_score <= 900 { kata_score } else { kata_score + 1400 };
        let eng = self.dict.english_for(&kata);
        let eng: Vec<&str> = eng
            .into_iter()
            .filter(|w| self.dict.english_rank(w).map_or(true, |r| r < 30000))
            .collect();
        for (i, w) in eng.iter().take(3).enumerate() {
            let learned = self.history.bonus(reading, w);
            let rank_pen = (self.dict.english_rank(w).unwrap_or(0) / 30).min(1000) as i32;
            put(&mut cands, w.to_string(), Kind::English, kata_score + 250 + 450 * i as i32 + rank_pen + learned);
            // capitalised form for proper-noun use is available one rank lower
            let mut cap = w.to_string();
            if let Some(f) = cap.get_mut(0..1) {
                f.make_ascii_uppercase();
            }
            put(&mut cands, cap, Kind::English, kata_score + 700 + 450 * i as i32 + rank_pen + learned);
        }

        // ---- literal keystrokes ----------------------------------------------------------
        if let Some(r) = raw {
            if r.chars().any(|c| c.is_ascii_alphabetic()) && r.is_ascii() {
                put(&mut cands, r.to_string(), Kind::Raw, 4200);
                put(&mut cands, to_fullwidth_ascii(r), Kind::FullAlpha, 13000);
            }
        }

        // learning: boost anything the user picked before for this exact reading
        let mut list: Vec<Candidate> = cands.into_values().collect();
        for c in list.iter_mut() {
            c.score += self.history.bonus(reading, &c.text);
        }
        list.sort_by(|a, b| a.score.cmp(&b.score).then(a.text.cmp(&b.text)));
        Segment { reading: reading.to_string(), candidates: list }
    }

    /// If the keystrokes are an English word (or clearly not Japanese), return a
    /// one-segment result with the literal word first.
    fn english_whole(&self, raw: &str, kana: &str) -> Option<Segment> {
        if raw.len() < 2 || !raw.bytes().all(|b| b.is_ascii_alphabetic()) {
            return None;
        }
        let lower = raw.to_ascii_lowercase();
        let rank = self.dict.english_rank(&lower);
        let leftover_ascii = kana.chars().any(|c| c.is_ascii_alphabetic());
        let has_upper = raw.chars().any(|c| c.is_ascii_uppercase());
        // typed with capitals or romaji that cannot be kana → literal input
        let literal = leftover_ascii || (has_upper && rank.is_some());
        // a common English word that is not a clean Japanese reading
        let chars: Vec<char> = kana.chars().collect();
        let bonus = |r: &str, s: &str| self.history.bonus(r, s);
        let user = |r: &str| self.user_nodes(r);
        let opts = BuildOpts { bonus: &bonus, user: &user, extra: &[] };
        let lat = Lattice::build(&self.dict, &chars, 0, chars.len(), 0, 0, &opts);
        let best = lat.nbest(&self.dict, 1);
        let jp_clean = best.first().map_or(false, |p| p.nodes.iter().all(|&i| !lat.nodes[i].synthetic));
        let wordy = rank.map_or(false, |r| r < 40000) && lower.len() >= 3;
        if !(literal && (rank.is_some() || leftover_ascii) || (wordy && !jp_clean)) {
            return None;
        }
        // Compose candidates: raw first, then the Japanese reading of the same keys
        let mut seg = self.segment_candidates(&chars, 0, chars.len(), 0, 0, kana, None, &opts);
        let mut list: Vec<Candidate> = Vec::new();
        let hist = self.history.bonus(kana, raw);
        list.push(Candidate { text: raw.to_string(), kind: Kind::Raw, score: -4000 + hist });
        if lower != raw {
            list.push(Candidate { text: lower.clone(), kind: Kind::Raw, score: -3900 });
        }
        let mut cap = lower.clone();
        if let Some(f) = cap.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        if cap != raw {
            list.push(Candidate { text: cap, kind: Kind::Raw, score: -3800 });
        }
        for c in seg.candidates.drain(..) {
            if !list.iter().any(|x| x.text == c.text) {
                list.push(c);
            }
        }
        // Japanese interpretation stays reachable but below English
        for c in list.iter_mut().skip(1) {
            if c.score < -3000 {
                continue;
            }
            c.score = c.score.max(-3000) + 1200;
        }
        list.sort_by(|a, b| a.score.cmp(&b.score).then(a.text.cmp(&b.text)));
        Some(Segment { reading: kana.to_string(), candidates: list })
    }

    /// Completion candidates while the user is still typing.
    pub fn predict(&self, raw: &str, n: usize) -> Vec<Candidate> {
        let (mut kana, pending) = self.romaji.convert(raw, false);
        if pending == "n" {
            kana.push('ん');
        }
        if kana.chars().count() < 2 || kana.chars().any(|c| c.is_ascii()) {
            return Vec::new();
        }
        let kata = hira_to_kata(&kana);
        let mut out: HashMap<String, Candidate> = HashMap::new();
        for (rd, entries) in self.dict.predict(&kana, 4000) {
            let extra = rd.chars().count() as i32 - kana.chars().count() as i32;
            if extra > 6 {
                continue;
            }
            for en in entries.iter().take(3) {
                let score = en.cost as i32 + 350 * extra + self.history.bonus(&rd, en.surface);
                out.entry(en.surface.to_string())
                    .and_modify(|c| c.score = c.score.min(score))
                    .or_insert(Candidate { text: en.surface.to_string(), kind: Kind::Predict, score });
            }
        }
        for (k, w) in self.dict.english_predict(&kata, 200) {
            let extra = k.chars().count() as i32 - kata.chars().count() as i32;
            if extra > 6 {
                continue;
            }
            let rank = self.dict.english_rank(w).unwrap_or(100000) as i32;
            let score = 3800 + 350 * extra + (rank / 400).min(2500);
            out.entry(w.to_string())
                .and_modify(|c| c.score = c.score.min(score))
                .or_insert(Candidate { text: w.to_string(), kind: Kind::English, score });
            let kt = k.clone();
            out.entry(kt.clone()).or_insert(Candidate { text: kt, kind: Kind::Predict, score: score - 200 });
        }
        let mut v: Vec<Candidate> = out.into_values().collect();
        v.sort_by(|a, b| a.score.cmp(&b.score).then(a.text.cmp(&b.text)));
        v.truncate(n);
        v
    }

    /// Learn from a committed selection (call once per segment).
    pub fn learn(&mut self, reading: &str, surface: &str) {
        if reading != surface {
            self.history.record(reading, surface);
        }
    }

    pub fn save(&mut self) {
        self.history.save();
    }
}

impl Engine {
    /// Debug aid: whole-sentence N-best with per-node costs.
    pub fn debug_paths(&self, raw: &str, n: usize) -> Vec<String> {
        let (kana, _) = self.romaji.convert(raw, true);
        let chars: Vec<char> = kana.chars().collect();
        let bonus = |r: &str, s: &str| self.history.bonus(r, s);
        let user = |r: &str| self.user_nodes(r);
        let opts = BuildOpts { bonus: &bonus, user: &user, extra: &[] };
        let lat = Lattice::build(&self.dict, &chars, 0, chars.len(), 0, 0, &opts);
        lat.nbest(&self.dict, n)
            .iter()
            .map(|p| {
                let parts: Vec<String> = p
                    .nodes
                    .iter()
                    .map(|&i| format!("{}({}{})", lat.nodes[i].surface, lat.nodes[i].wcost, if lat.nodes[i].synthetic { "*" } else { "" }))
                    .collect();
                format!("{:>6}  {}", p.cost, parts.join(" "))
            })
            .collect()
    }
}

impl Engine {
    fn looks_english(&self, word: &str) -> bool {
        let lower = word.to_ascii_lowercase();
        techwords::contains(&lower)
            || (lower.len() >= 4 && self.dict.english_rank(&lower).map_or(false, |r| r < 20000))
    }

    /// English-looking letter runs in `raw` whose edges do not fall on kana boundaries.
    fn unaligned_english(&self, raw: &str, chunks: &[romaji::Chunk]) -> Vec<(usize, usize, i32)> {
        let rawc: Vec<char> = raw.chars().collect();
        let mut bounds: Vec<usize> = chunks.iter().map(|c| c.start).collect();
        if let Some(l) = chunks.last() {
            bounds.push(l.end);
        }
        let aligned = |p: usize| bounds.contains(&p);
        let mut out: Vec<(usize, usize, i32)> = Vec::new();
        for i in 0..rawc.len() {
            if !rawc[i].is_ascii_alphabetic() {
                continue;
            }
            for j in (i + 3..=rawc.len().min(i + 24)).rev() {
                if !rawc[i..j].iter().all(|c| c.is_ascii_alphabetic()) {
                    break;
                }
                if aligned(i) && aligned(j) {
                    continue;
                }
                let w: String = rawc[i..j].iter().collect();
                if self.looks_english(&w) {
                    let lower = w.to_ascii_lowercase();
                    let confident = techwords::contains(&lower)
                        || rawc[i].is_ascii_uppercase()
                        || self.dict.english_rank(&lower).map_or(false, |r| r < 8000);
                    out.push((i, j, if confident { 1500 } else { 0 }));
                }
            }
        }
        out.sort_by_key(|&(i, j, _)| std::cmp::Reverse(j - i));
        out
    }
}
