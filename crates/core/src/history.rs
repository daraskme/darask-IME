//! User learning: remembers which surface was chosen for which reading.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct History {
    map: HashMap<(String, String), u32>,
    by_r: HashMap<String, Vec<String>>,
    path: Option<PathBuf>,
    dirty: bool,
}

impl History {
    pub fn load(path: &Path) -> History {
        let mut h = History { path: Some(path.to_path_buf()), ..Default::default() };
        if let Ok(t) = fs::read_to_string(path) {
            for l in t.lines() {
                let f: Vec<&str> = l.split('\t').collect();
                if f.len() == 3 {
                    if let Ok(c) = f[2].parse() {
                        h.by_r.entry(f[0].to_string()).or_default().push(f[1].to_string());
                        h.map.insert((f[0].to_string(), f[1].to_string()), c);
                    }
                }
            }
        }
        h
    }

    pub fn in_memory() -> History {
        History::default()
    }

    pub fn count(&self, reading: &str, surface: &str) -> u32 {
        self.map.get(&(reading.to_string(), surface.to_string())).copied().unwrap_or(0)
    }

    /// Negative cost adjustment (more negative = stronger preference).
    pub fn bonus(&self, reading: &str, surface: &str) -> i32 {
        let c = self.count(reading, surface);
        if c == 0 {
            0
        } else {
            -((1400.0 * (1.0 + c as f64).ln()) as i32).min(5000)
        }
    }

    pub fn record(&mut self, reading: &str, surface: &str) {
        if !self.map.contains_key(&(reading.to_string(), surface.to_string())) {
            self.by_r.entry(reading.to_string()).or_default().push(surface.to_string());
        }
        *self.map.entry((reading.to_string(), surface.to_string())).or_insert(0) += 1;
        self.dirty = true;
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        if let Some(p) = &self.path {
            if let Some(d) = p.parent() {
                let _ = fs::create_dir_all(d);
            }
            let mut s = String::new();
            for ((r, w), c) in &self.map {
                s.push_str(&format!("{}\t{}\t{}\n", r, w, c));
            }
            let tmp = p.with_extension("tmp");
            if fs::write(&tmp, s).is_ok() {
                let _ = fs::rename(&tmp, p);
            }
            self.dirty = false;
        }
    }
}

impl History {
    /// Everything learned for exactly this reading, as (surface, count).
    pub fn by_reading(&self, reading: &str) -> Vec<(String, u32)> {
        self.by_r
            .get(reading)
            .map(|v| v.iter().map(|s| (s.clone(), self.count(reading, s))).collect())
            .unwrap_or_default()
    }
}
