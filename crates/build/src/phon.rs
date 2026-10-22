//! English (CMUdict ARPAbet) → katakana readings.
//! Produces several plausible katakana spellings per pronunciation so that a
//! user typing a loanword reading hits the English word.

const MAX_VARIANTS: usize = 8;

fn is_vowel(p: &str) -> bool {
    matches!(
        p,
        "AA" | "AE" | "AH" | "AO" | "AW" | "AY" | "EH" | "ER" | "EY" | "IH" | "IY" | "OW" | "OY" | "UH" | "UW"
    )
}

/// vowel index: a i u e o
fn row(c: &str) -> Option<[&'static str; 5]> {
    Some(match c {
        "K" => ["カ", "キ", "ク", "ケ", "コ"],
        "G" => ["ガ", "ギ", "グ", "ゲ", "ゴ"],
        "S" => ["サ", "シ", "ス", "セ", "ソ"],
        "Z" => ["ザ", "ジ", "ズ", "ゼ", "ゾ"],
        "T" => ["タ", "ティ", "トゥ", "テ", "ト"],
        "D" => ["ダ", "ディ", "ドゥ", "デ", "ド"],
        "N" => ["ナ", "ニ", "ヌ", "ネ", "ノ"],
        "HH" => ["ハ", "ヒ", "フ", "ヘ", "ホ"],
        "P" => ["パ", "ピ", "プ", "ペ", "ポ"],
        "B" => ["バ", "ビ", "ブ", "ベ", "ボ"],
        "M" => ["マ", "ミ", "ム", "メ", "モ"],
        "R" | "L" => ["ラ", "リ", "ル", "レ", "ロ"],
        "F" => ["ファ", "フィ", "フ", "フェ", "フォ"],
        "V" => ["ヴァ", "ヴィ", "ヴ", "ヴェ", "ヴォ"],
        "TH" => ["サ", "シ", "ス", "セ", "ソ"],
        "DH" => ["ザ", "ジ", "ズ", "ゼ", "ゾ"],
        "SH" => ["シャ", "シ", "シュ", "シェ", "ショ"],
        "ZH" | "JH" => ["ジャ", "ジ", "ジュ", "ジェ", "ジョ"],
        "CH" => ["チャ", "チ", "チュ", "チェ", "チョ"],
        "W" => ["ワ", "ウィ", "ウ", "ウェ", "ウォ"],
        "Y" => ["ヤ", "イ", "ユ", "イェ", "ヨ"],
        _ => return None,
    })
}

/// Epenthetic vowel-less rendering of a consonant (used when no vowel follows).
fn bare(c: &str) -> &'static str {
    match c {
        "K" => "ク",
        "G" => "グ",
        "S" => "ス",
        "Z" => "ズ",
        "T" => "ト",
        "D" => "ド",
        "N" => "ン",
        "M" => "ム",
        "HH" => "フ",
        "P" => "プ",
        "B" => "ブ",
        "R" => "ル",
        "L" => "ル",
        "F" => "フ",
        "V" => "ブ",
        "TH" => "ス",
        "DH" => "ズ",
        "SH" => "シュ",
        "ZH" | "JH" => "ジ",
        "CH" => "チ",
        "W" => "ウ",
        "Y" => "イ",
        "NG" => "ング",
        _ => "",
    }
}

fn vowel_idx(v: &str) -> usize {
    match v {
        "AA" | "AE" | "AH" | "AY" | "AW" | "ER" => 0,
        "IH" | "IY" | "EY" => 3, // overridden below by caller where needed
        _ => 0,
    }
}

/// Standalone and post-consonant renderings of vowels: (alternatives, row index for C+V merge).
/// Returns list of (full kana when standalone, kana suffix after consonant, merge row idx).
fn vowel_alts(v: &str, stressed_initial: bool) -> Vec<(&'static str, usize, &'static str)> {
    // (standalone, row index to merge into consonant, trailing extra kana)
    match v {
        "AA" => vec![("ア", 0, ""), ("オ", 4, "")],
        "AE" => vec![("ア", 0, "")],
        "AH" => vec![("ア", 0, ""), ("オ", 4, "")],
        "AO" => vec![("オ", 4, "")],
        "AW" => vec![("ア", 0, "ウ")],
        "AY" => vec![("ア", 0, "イ")],
        "EH" => vec![("エ", 3, "")],
        "ER" => vec![("アー", 0, ""), ("ア", 0, "")],
        "EY" => vec![("エ", 3, "イ"), ("エ", 3, "ー")],
        "IH" => {
            if stressed_initial {
                vec![("イ", 1, ""), ("エ", 3, "")]
            } else {
                vec![("イ", 1, "")]
            }
        }
        "IY" => vec![("イ", 1, "ー"), ("イ", 1, "")],
        "OW" => vec![("オ", 4, "ー"), ("オ", 4, "ウ")],
        "OY" => vec![("オ", 4, "イ")],
        "UH" => vec![("ウ", 2, "")],
        "UW" => vec![("ウ", 2, "ー"), ("ウ", 2, "")],
        _ => vec![("", 0, "")],
    }
}

const VOWEL_KANA: [&str; 5] = ["ア", "イ", "ウ", "エ", "オ"];

fn strip_stress(p: &str) -> &str {
    p.trim_end_matches(|c: char| c.is_ascii_digit())
}

/// Convert one pronunciation to up to MAX_VARIANTS katakana spellings.
pub fn to_katakana(phones: &[&str]) -> Vec<String> {
    let ph: Vec<&str> = phones.iter().map(|p| strip_stress(p)).collect();
    let n = ph.len();
    let mut beams: Vec<String> = vec![String::new()];
    let mut i = 0;
    let _ = vowel_idx;
    while i < n {
        let p = ph[i];
        let next = ph.get(i + 1).copied();
        let mut opts: Vec<(String, usize)> = Vec::new(); // (text, consumed)
        if matches!(p, "T" | "D" | "N" | "HH" | "K" | "G" | "B" | "P" | "M" | "R" | "F" | "V")
            && next == Some("Y")
            && ph.get(i + 2).copied() == Some("UW")
        {
            // palatal onset: computer → コンピュータ, new → ニュー
            let base = match p {
                "T" => "テュ".to_string(),
                "D" => "デュ".to_string(),
                "F" => "フュ".to_string(),
                "V" => "ヴュ".to_string(),
                _ => format!("{}ュ", row(p).unwrap()[1]),
            };
            opts.push((format!("{}ー", base), 3));
            opts.push((base.clone(), 3));
            if p == "T" {
                opts.push(("チュー".to_string(), 3));
            }
            if p == "D" {
                opts.push(("ジュー".to_string(), 3));
            }
        } else if let Some(r) = row(p) {
            match next {
                Some(v) if is_vowel(v) => {
                    // r-colored final handled in vowel branch
                    for (_, idx, extra) in vowel_alts(v, i == 0) {
                        // ER after consonant → consonant + アー
                        if v == "ER" {
                            opts.push((format!("{}ー", r[0]), 2));
                            opts.push((r[0].to_string(), 2));
                            break;
                        }
                        opts.push((format!("{}{}", r[idx], extra), 2));
                    }
                    // -tion / -sion / -gion: SH AH N → ション
                    if v == "AH" && matches!(p, "SH" | "ZH" | "JH" | "CH") && ph.get(i + 2).copied() == Some("N") {
                        opts.insert(0, (r[4].to_string(), 2));
                    }
                    if opts.is_empty() {
                        opts.push((bare(p).to_string(), 1));
                    }
                }
                Some("NG") | Some(_) | None => {
                    let mut t = bare(p).to_string();
                    // N/M before another consonant → ン
                    if (p == "N" || p == "M") && next.map_or(false, |x| !is_vowel(x)) {
                        t = "ン".to_string();
                    }
                    // R at the end after a vowel is absorbed (car → カー)
                    if (p == "R") && i > 0 && is_vowel(ph[i - 1]) && next.is_none() {
                        t = "ー".to_string();
                    }
                    // Final -t/-d after T/D: ット is handled by sokuon in vowel branch
                    // L before consonant: ル
                    opts.push((t, 1));
                }
            }
        } else if p == "NG" {
            opts.push(("ン".to_string(), 1));
            // NG followed by vowel gets its own "ング" nuance
            if next.map_or(true, |x| !is_vowel(x)) {
                opts.clear();
                opts.push(("ング".to_string(), 1));
                opts.push(("ン".to_string(), 1));
            } else {
                opts.clear();
                opts.push(("ン".to_string(), 1));
            }
        } else if is_vowel(p) {
            // standalone vowel (word-initial or after vowel)
            let mut v_opts = vowel_alts(p, i == 0);
            if p == "ER" && i > 0 && ph.get(i - 1).map_or(false, |x| is_vowel(x)) {
                v_opts = vec![("ア", 0, "")];
            }
            for (s, _idx, extra) in v_opts {
                let mut t = format!("{}{}", s, extra);
                // sokuon: short vowel + final stop (cup→カップ, hit→ヒット)
                let short = matches!(p, "AE" | "AH" | "EH" | "IH" | "UH");
                if short && i + 2 == n {
                    if let Some(c) = next {
                        if matches!(c, "K" | "G" | "T" | "D" | "P" | "B" | "CH" | "JH") {
                            t.push('ッ');
                        }
                    }
                }
                opts.push((t, 1));
            }
        } else {
            opts.push((String::new(), 1));
        }
        // special: C+V merged cases need sokuon check too
        let mut new_beams: Vec<String> = Vec::new();
        for b in &beams {
            for (t, _) in &opts {
                if new_beams.len() >= MAX_VARIANTS * 2 {
                    break;
                }
                new_beams.push(format!("{}{}", b, t));
            }
        }
        let consumed = opts.first().map(|o| o.1).unwrap_or(1);
        // sokuon insertion for merged C+V followed by final stop
        if consumed == 2 && is_vowel(ph[i + 1]) {
            let v = ph[i + 1];
            let short = matches!(v, "AE" | "AH" | "EH" | "IH" | "UH");
            if short && i + 3 == n && matches!(ph[i + 2], "K" | "G" | "T" | "D" | "P" | "B" | "CH" | "JH") {
                for b in new_beams.iter_mut() {
                    b.push('ッ');
                }
            }
        }
        new_beams.truncate(MAX_VARIANTS);
        beams = new_beams;
        i += consumed;
    }
    let _ = VOWEL_KANA;
    beams.retain(|b| !b.is_empty());
    beams.sort();
    beams.dedup();
    beams
}

