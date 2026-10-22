//! Per-input-context composition state machine.
//!
//! Frontend-agnostic: the fcitx5 addon (or any other frontend) feeds key events
//! in and reads preedit / candidate list / commit text out.

use crate::{kana, Candidate, Engine, Kind, Segment};

pub const PAGE: usize = 9;

// X11 keysyms (fcitx5 uses the same values)
pub mod sym {
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const RETURN: u32 = 0xff0d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const HENKAN: u32 = 0xff23;
    pub const HOME: u32 = 0xff50;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
    pub const PAGE_UP: u32 = 0xff55;
    pub const PAGE_DOWN: u32 = 0xff56;
    pub const END: u32 = 0xff57;
    pub const KP_ENTER: u32 = 0xff8d;
    pub const F6: u32 = 0xffc3;
    pub const F7: u32 = 0xffc4;
    pub const F8: u32 = 0xffc5;
    pub const F9: u32 = 0xffc6;
    pub const F10: u32 = 0xffc7;
    pub const DELETE: u32 = 0xffff;
    pub const SPACE: u32 = 0x20;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Key {
    pub sym: u32,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Idle,
    Composing,
    Converting,
}

pub struct Session {
    mode: Mode,
    raw: String,
    segs: Vec<Segment>,
    spans: Vec<(usize, usize)>,
    sel: Vec<usize>,
    focus: usize,
    /// suggestions shown while composing
    sugg: Vec<Candidate>,
    sugg_sel: Option<usize>,
    commit: String,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Session {
        Session { mode: Mode::Idle, raw: String::new(), segs: vec![], spans: vec![], sel: vec![], focus: 0, sugg: vec![], sugg_sel: None, commit: String::new() }
    }

    pub fn reset(&mut self) {
        *self = Session::new();
    }

    pub fn is_active(&self) -> bool {
        self.mode != Mode::Idle
    }

    pub fn take_commit(&mut self) -> Option<String> {
        if self.commit.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.commit))
        }
    }

    /// Preedit as (text, highlighted) pieces.
    pub fn preedit(&self, eng: &Engine) -> Vec<(String, bool)> {
        match self.mode {
            Mode::Idle => vec![],
            Mode::Composing => {
                if let Some(i) = self.sugg_sel {
                    return vec![(self.sugg[i].text.clone(), true)];
                }
                let (k, p) = eng.preedit(&self.raw);
                vec![(format!("{}{}", k, p), false)]
            }
            Mode::Converting => self
                .segs
                .iter()
                .enumerate()
                .map(|(i, s)| (s.candidates[self.sel[i]].text.clone(), i == self.focus))
                .collect(),
        }
    }

    /// Candidates to display: (list, cursor index in the whole list).
    pub fn candidates(&self) -> (Vec<Candidate>, Option<usize>) {
        match self.mode {
            Mode::Idle => (vec![], None),
            Mode::Composing => (self.sugg.clone(), self.sugg_sel),
            Mode::Converting => (self.segs[self.focus].candidates.clone(), Some(self.sel[self.focus])),
        }
    }

    pub fn key(&mut self, eng: &mut Engine, k: Key) -> bool {
        if k.ctrl || k.alt {
            // let shortcuts through; commit what we have first so nothing is lost
            if self.mode != Mode::Idle {
                self.commit_all(eng);
            }
            return false;
        }
        match self.mode {
            Mode::Idle => self.key_idle(eng, k),
            Mode::Composing => self.key_composing(eng, k),
            Mode::Converting => self.key_converting(eng, k),
        }
    }

    fn printable(k: Key) -> Option<char> {
        (0x21..=0x7e).contains(&k.sym).then(|| char::from_u32(k.sym).unwrap())
    }

    fn key_idle(&mut self, eng: &mut Engine, k: Key) -> bool {
        if let Some(c) = Self::printable(k) {
            self.raw.push(c);
            self.mode = Mode::Composing;
            self.refresh(eng);
            return true;
        }
        false
    }

    fn key_composing(&mut self, eng: &mut Engine, k: Key) -> bool {
        use sym::*;
        match k.sym {
            BACKSPACE => {
                if self.sugg_sel.take().is_some() {
                    return true;
                }
                self.raw.pop();
                if self.raw.is_empty() {
                    self.reset();
                } else {
                    self.refresh(eng);
                }
            }
            ESCAPE => {
                if self.sugg_sel.take().is_none() {
                    self.reset();
                }
            }
            RETURN | KP_ENTER => {
                if let Some(i) = self.sugg_sel {
                    let c = self.sugg[i].clone();
                    let (kana, _) = eng.romaji.convert(&self.raw, true);
                    eng.learn(&kana, &c.text);
                    self.commit = c.text;
                } else {
                    self.commit = eng.romaji.convert(&self.raw, true).0;
                }
                eng.save();
                self.reset_keep_commit();
            }
            SPACE | HENKAN => self.start_conversion(eng),
            TAB | DOWN => {
                if !self.sugg.is_empty() {
                    let n = self.sugg.len();
                    self.sugg_sel = Some(match self.sugg_sel {
                        None => 0,
                        Some(i) => (i + 1) % n,
                    });
                }
            }
            UP => {
                if !self.sugg.is_empty() {
                    let n = self.sugg.len();
                    self.sugg_sel = Some(match self.sugg_sel {
                        None | Some(0) => n - 1,
                        Some(i) => i - 1,
                    });
                }
            }
            F6 | F7 | F8 | F9 | F10 => self.function_key(eng, k.sym),
            c if Self::printable(k).is_some() => {
                if let Some(i) = self.sugg_sel {
                    // typing while a suggestion is highlighted: accept it, start anew
                    let t = self.sugg[i].text.clone();
                    let (kana, _) = eng.romaji.convert(&self.raw, true);
                    eng.learn(&kana, &t);
                    self.commit = t;
                    self.raw.clear();
                    self.sugg_sel = None;
                }
                self.raw.push(char::from_u32(c).unwrap());
                self.refresh(eng);
            }
            _ => return true, // swallow cursor keys etc. while composing
        }
        true
    }

    fn reset_keep_commit(&mut self) {
        let c = std::mem::take(&mut self.commit);
        self.reset();
        self.commit = c;
    }

    fn function_key(&mut self, eng: &mut Engine, s: u32) {
        let (kana, _) = eng.romaji.convert(&self.raw, true);
        let text = match s {
            sym::F6 => kana::kata_to_hira(&kana),
            sym::F7 => kana::hira_to_kata(&kana),
            sym::F8 => kana::to_halfwidth_kana(&kana),
            sym::F9 => kana::to_fullwidth_ascii(&self.raw),
            _ => self.raw.clone(),
        };
        self.segs = vec![Segment {
            reading: kana,
            candidates: vec![Candidate { text, kind: Kind::Raw, score: 0 }],
        }];
        self.sel = vec![0];
        self.focus = 0;
        self.mode = Mode::Converting;
    }

    /// Shift+Right / Shift+Left: move the boundary of the focused segment.
    fn resize(&mut self, eng: &mut Engine, grow: bool) {
        if self.spans.len() != self.segs.len() {
            return;
        }
        let f = self.focus;
        let (s, e) = self.spans[f];
        if grow {
            if f + 1 >= self.spans.len() {
                return;
            }
            let (_, e2) = self.spans[f + 1];
            self.spans[f].1 = e + 1;
            if e + 1 >= e2 {
                self.spans.remove(f + 1);
            } else {
                self.spans[f + 1].0 = e + 1;
            }
        } else {
            if e - s <= 1 {
                return;
            }
            self.spans[f].1 = e - 1;
            if f + 1 < self.spans.len() {
                self.spans[f + 1].0 = e - 1;
            } else {
                self.spans.push((e - 1, e));
            }
        }
        let (segs, spans) = eng.convert_spans(&self.raw, Some(&self.spans));
        self.sel = vec![0; segs.len()];
        self.segs = segs;
        self.spans = spans;
        self.focus = f.min(self.segs.len() - 1);
    }

    fn start_conversion(&mut self, eng: &mut Engine) {
        let (segs, spans) = eng.convert_spans(&self.raw, None);
        if segs.is_empty() {
            self.reset();
            return;
        }
        self.sel = vec![0; segs.len()];
        self.segs = segs;
        self.spans = spans;
        self.focus = 0;
        self.sugg_sel = None;
        self.mode = Mode::Converting;
    }

    fn key_converting(&mut self, eng: &mut Engine, k: Key) -> bool {
        use sym::*;
        let n = self.segs[self.focus].candidates.len();
        match k.sym {
            SPACE | DOWN | TAB | HENKAN => self.sel[self.focus] = (self.sel[self.focus] + 1) % n,
            UP => self.sel[self.focus] = (self.sel[self.focus] + n - 1) % n,
            PAGE_DOWN => self.sel[self.focus] = (self.sel[self.focus] + PAGE).min(n - 1),
            PAGE_UP => self.sel[self.focus] = self.sel[self.focus].saturating_sub(PAGE),
            RIGHT if k.shift => self.resize(eng, true),
            LEFT if k.shift => self.resize(eng, false),
            RIGHT => {
                if self.focus + 1 < self.segs.len() {
                    self.focus += 1;
                }
            }
            LEFT => self.focus = self.focus.saturating_sub(1),
            RETURN | KP_ENTER => self.commit_all(eng),
            ESCAPE | BACKSPACE => {
                self.mode = Mode::Composing;
                self.segs.clear();
                self.sel.clear();
                self.refresh(eng);
            }
            F6 | F7 | F8 | F9 | F10 => self.function_key(eng, k.sym),
            c if (0x31..=0x39).contains(&c) => {
                let page_start = self.sel[self.focus] / PAGE * PAGE;
                let idx = page_start + (c - 0x31) as usize;
                if idx < n {
                    self.sel[self.focus] = idx;
                    if self.focus + 1 < self.segs.len() {
                        self.focus += 1;
                    } else {
                        self.commit_all(eng);
                    }
                }
            }
            _ => {
                if Self::printable(k).is_some() {
                    self.commit_all(eng);
                    return self.key_idle(eng, k);
                }
                return true;
            }
        }
        true
    }

    /// Select candidate `index` of the visible list (mouse click or label key).
    pub fn select(&mut self, eng: &mut Engine, index: usize) {
        match self.mode {
            Mode::Converting => {
                if index < self.segs[self.focus].candidates.len() {
                    self.sel[self.focus] = index;
                    if self.focus + 1 < self.segs.len() {
                        self.focus += 1;
                    } else {
                        self.commit_all(eng);
                    }
                }
            }
            Mode::Composing => {
                if index < self.sugg.len() {
                    self.sugg_sel = Some(index);
                    let c = self.sugg[index].clone();
                    let (kana, _) = eng.romaji.convert(&self.raw, true);
                    eng.learn(&kana, &c.text);
                    self.commit = c.text;
                    eng.save();
                    self.reset_keep_commit();
                }
            }
            Mode::Idle => {}
        }
    }

    fn commit_all(&mut self, eng: &mut Engine) {
        match self.mode {
            Mode::Converting => {
                let mut out = String::new();
                for (i, s) in self.segs.iter().enumerate() {
                    let t = &s.candidates[self.sel[i]].text;
                    eng.learn(&s.reading, t);
                    out.push_str(t);
                }
                self.commit = out;
                eng.save();
            }
            Mode::Composing => {
                self.commit = eng.romaji.convert(&self.raw, true).0;
            }
            Mode::Idle => {}
        }
        self.reset_keep_commit();
    }

    /// Recompute the suggestion list for the current raw input.
    fn refresh(&mut self, eng: &Engine) {
        self.sugg_sel = None;
        let mut list: Vec<Candidate> = Vec::new();
        let segs = eng.convert(&self.raw);
        if segs.len() == 1 {
            list.extend(segs[0].candidates.iter().take(6).cloned());
        } else if !segs.is_empty() {
            let whole: String = segs.iter().map(|s| s.candidates[0].text.as_str()).collect();
            list.push(Candidate { text: whole, kind: Kind::Conversion, score: 0 });
        }
        for c in eng.predict(&self.raw, 8) {
            if !list.iter().any(|x| x.text == c.text) {
                list.push(c);
            }
        }
        // keep the plain hiragana reading out of the list's first slot unless it is all we have
        list.truncate(12);
        self.sugg = list;
    }
}
