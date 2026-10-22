//! Technical vocabulary missing from general English frequency lists.

use std::collections::HashSet;
use std::sync::OnceLock;

static WORDS: OnceLock<HashSet<&'static str>> = OnceLock::new();

pub fn contains(w: &str) -> bool {
    WORDS
        .get_or_init(|| include_str!("../tech_words.txt").split_whitespace().collect())
        .contains(w)
}
