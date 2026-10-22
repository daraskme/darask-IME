//! Word lattice + Viterbi / A* N-best over a hiragana reading.

use crate::dict::Dict;
use crate::kana::{self, foreign_score, hira_to_kata};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

#[derive(Clone, Debug)]
pub struct Node {
    pub start: usize, // char index
    pub end: usize,
    pub lid: u16,
    pub rid: u16,
    pub wcost: i32,
    pub surface: String,
    /// true for nodes that do not come from the dictionary
    pub synthetic: bool,
}

pub struct Path {
    pub nodes: Vec<usize>, // indices into Lattice::nodes
    pub cost: i32,
}

pub struct Lattice {
    pub nodes: Vec<Node>,
    ends_at: Vec<Vec<usize>>,
    starts_at: Vec<Vec<usize>>,
    s: usize,
    e: usize,
    left_rid: u16,
    right_lid: u16,
    fwd: Vec<i32>,
}

pub struct BuildOpts<'a> {
    /// extra (negative) cost for a (reading, surface) pair the user chose before
    pub bonus: &'a dyn Fn(&str, &str) -> i32,
    /// words the user taught us: reading -> [(surface, cost)]
    pub user: &'a dyn Fn(&str) -> Vec<(String, i32)>,
    /// extra nodes (e.g. English words found in the raw keystrokes)
    pub extra: &'a [Node],
}

fn is_digit_like(c: char) -> bool {
    c.is_ascii_digit()
}

impl Lattice {
    /// Build a lattice for chars[s..e] of `reading`.
    pub fn build(
        dict: &Dict,
        chars: &[char],
        s: usize,
        e: usize,
        left_rid: u16,
        right_lid: u16,
        opts: &BuildOpts,
    ) -> Lattice {
        let n = chars.len();
        let mut nodes: Vec<Node> = Vec::new();
        let mut ends_at = vec![Vec::new(); n + 1];
        let mut starts_at = vec![Vec::new(); n + 1];
        // byte offsets per char index for slicing
        let mut boff = Vec::with_capacity(n + 1);
        let mut acc = 0;
        for c in chars {
            boff.push(acc);
            acc += c.len_utf8();
        }
        boff.push(acc);
        let text: String = chars.iter().collect();

        let mut add = |nodes: &mut Vec<Node>, node: Node| {
            let id = nodes.len();
            ends_at[node.end].push(id);
            starts_at[node.start].push(id);
            nodes.push(node);
        };

        for n in opts.extra {
            if n.start >= s && n.end <= e {
                add(&mut nodes, n.clone());
            }
        }
        for p in s..e {
            let tail = &text[boff[p]..boff[e]];
            let mut covered_noun_kata: HashSet<usize> = HashSet::new();
            for (blen, entries) in dict.prefixes(tail) {
                let end = p + tail[..blen].chars().count();
                let reading = &text[boff[p]..boff[end]];
                for en in entries {
                    let b = (opts.bonus)(reading, en.surface);
                    add(
                        &mut nodes,
                        Node {
                            start: p,
                            end,
                            lid: en.lid,
                            rid: en.rid,
                            wcost: en.cost as i32 + b,
                            surface: en.surface.to_string(),
                            synthetic: false,
                        },
                    );
                    if kana::is_katakana(en.surface.chars().next().unwrap_or('a')) {
                        covered_noun_kata.insert(end);
                    }
                }
            }
            for j in p + 1..=e.min(p + 30) {
                let reading = &text[boff[p]..boff[j]];
                for (surf, cost) in (opts.user)(reading) {
                    add(
                        &mut nodes,
                        Node { start: p, end: j, lid: dict.noun_general, rid: dict.noun_general, wcost: cost, surface: surf, synthetic: false },
                    );
                }
            }
            let c0 = chars[p];
            if kana::is_hiragana(c0) {
                // unknown-word fallback #1: single hiragana, always keeps the graph connected
                add(
                    &mut nodes,
                    Node {
                        start: p,
                        end: p + 1,
                        lid: dict.noun_general,
                        rid: dict.noun_general,
                        wcost: 11000,
                        surface: c0.to_string(),
                        synthetic: true,
                    },
                );
                // unknown-word fallback #2: katakana rendering of a kana run. Cheap when it
                // contains loanword-only sounds (ふぇ, てぃ, ゔ …), expensive otherwise.
                let mut j = p;
                while j < e && j - p < 14 && (kana::is_hiragana(chars[j])) {
                    j += 1;
                    let reading = &text[boff[p]..boff[j]];
                    let len = j - p;
                    let fs = foreign_score(reading) as i32;
                    let fchars = (2 * fs as usize).min(len) as i32;
                    let mut cost = 5000 + 2400 * (len as i32 - fchars) + 700 * fchars;
                    if reading.chars().all(|c| c == 'ー') {
                        continue;
                    }
                    // a run made only of long marks or single "ー" is not a word
                    if len == 1 && fs == 0 {
                        cost += 3000;
                    }
                    if covered_noun_kata.contains(&j) {
                        continue; // dictionary already offers a katakana word here
                    }
                    let kata = hira_to_kata(reading);
                    cost += (opts.bonus)(reading, &kata);
                    add(
                        &mut nodes,
                        Node {
                            start: p,
                            end: j,
                            lid: dict.noun_general,
                            rid: dict.noun_general,
                            wcost: cost,
                            surface: kata,
                            synthetic: true,
                        },
                    );
                }
            } else if kana::is_katakana(c0) {
                // already katakana input: keep as is
                let mut j = p;
                while j < e && kana::is_katakana(chars[j]) && j - p < 20 {
                    j += 1;
                    add(
                        &mut nodes,
                        Node {
                            start: p,
                            end: j,
                            lid: dict.noun_general,
                            rid: dict.noun_general,
                            wcost: 5000 + 400 * (j - p) as i32,
                            surface: chars[p..j].iter().collect(),
                            synthetic: true,
                        },
                    );
                }
            } else {
                // ascii / digits / symbols: one node per maximal run of same class
                let digit = is_digit_like(c0);
                let mut j = p + 1;
                while j < e
                    && !kana::is_hiragana(chars[j])
                    && !kana::is_katakana(chars[j])
                    && is_digit_like(chars[j]) == digit
                {
                    j += 1;
                }
                add(
                    &mut nodes,
                    Node {
                        start: p,
                        end: j,
                        lid: if digit { dict.noun_general } else { dict.symbol_general },
                        rid: if digit { dict.noun_general } else { dict.symbol_general },
                        wcost: 3000,
                        surface: chars[p..j].iter().collect(),
                        synthetic: true,
                    },
                );
                if j - p > 1 {
                    add(
                        &mut nodes,
                        Node {
                            start: p,
                            end: p + 1,
                            lid: dict.symbol_general,
                            rid: dict.symbol_general,
                            wcost: 4000,
                            surface: chars[p].to_string(),
                            synthetic: true,
                        },
                    );
                }
            }
        }

        let mut lat = Lattice {
            fwd: vec![i32::MAX; nodes.len()],
            nodes,
            ends_at,
            starts_at,
            s,
            e,
            left_rid,
            right_lid,
        };
        lat.forward(dict);
        lat
    }

    fn forward(&mut self, dict: &Dict) {
        for p in self.s..self.e {
            let ids = self.starts_at[p].clone();
            for id in ids {
                let n = &self.nodes[id];
                let best = if p == self.s {
                    dict.conn(self.left_rid, n.lid)
                } else {
                    let mut b = i32::MAX;
                    for &pr in &self.ends_at[p] {
                        if self.fwd[pr] == i32::MAX {
                            continue;
                        }
                        let c = self.fwd[pr] + dict.conn(self.nodes[pr].rid, n.lid);
                        b = b.min(c);
                    }
                    b
                };
                if best != i32::MAX {
                    self.fwd[id] = best + n.wcost;
                }
            }
        }
    }

    /// Up to `n` best distinct paths (by surface string).
    pub fn nbest(&self, dict: &Dict, n: usize) -> Vec<Path> {
        let mut heap: BinaryHeap<Reverse<(i32, i32, usize, Vec<usize>)>> = BinaryHeap::new();
        // state: (priority, g_after, node, path-from-node-to-end)
        for &id in &self.ends_at[self.e] {
            if self.fwd[id] == i32::MAX {
                continue;
            }
            let g = dict.conn(self.nodes[id].rid, self.right_lid);
            heap.push(Reverse((self.fwd[id] + g, g, id, vec![id])));
        }
        let mut out: Vec<Path> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut pops = 0;
        while let Some(Reverse((prio, g, id, path))) = heap.pop() {
            pops += 1;
            if pops > 4000 || out.len() >= n {
                break;
            }
            let node = &self.nodes[id];
            if node.start == self.s {
                let key: String = path.iter().map(|&i| self.nodes[i].surface.as_str()).collect::<Vec<_>>().join("\u{1}");
                if seen.insert(key) {
                    out.push(Path { nodes: path, cost: prio });
                }
                continue;
            }
            for &pr in &self.ends_at[node.start] {
                if self.fwd[pr] == i32::MAX {
                    continue;
                }
                let g2 = g + node.wcost + dict.conn(self.nodes[pr].rid, node.lid);
                let mut p2 = Vec::with_capacity(path.len() + 1);
                p2.push(pr);
                p2.extend_from_slice(&path);
                heap.push(Reverse((self.fwd[pr] + g2, g2, pr, p2)));
            }
        }
        out
    }
}
