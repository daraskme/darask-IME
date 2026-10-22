//! Small kana/width helpers.

pub fn is_hiragana(c: char) -> bool {
    ('ぁ'..='ゖ').contains(&c) || c == 'ー' || c == 'ゝ' || c == 'ゞ'
}

pub fn is_katakana(c: char) -> bool {
    ('ァ'..='ヺ').contains(&c) || c == 'ー'
}

pub fn is_kana_str(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| is_hiragana(c) || is_katakana(c))
}

pub fn hira_to_kata(s: &str) -> String {
    s.chars()
        .map(|c| if ('ぁ'..='ゖ').contains(&c) { char::from_u32(c as u32 + 0x60).unwrap() } else { c })
        .collect()
}

pub fn kata_to_hira(s: &str) -> String {
    s.chars()
        .map(|c| if ('ァ'..='ヶ').contains(&c) { char::from_u32(c as u32 - 0x60).unwrap() } else { c })
        .collect()
}

/// Kana sounds that, in practice, are only used for loanwords / onomatopoeia.
/// Strings containing them are very likely to be written in katakana.
const FOREIGN_SEQ: &[&str] = &[
    "てぃ", "でぃ", "とぅ", "どぅ", "ふぁ", "ふぃ", "ふぇ", "ふぉ", "うぃ", "うぇ", "うぉ", "しぇ", "じぇ", "ちぇ",
    "つぁ", "つぃ", "つぇ", "つぉ", "いぇ", "くぁ", "くぃ", "くぇ", "くぉ", "ぐぁ", "ゔ", "ヴ", "すぃ", "ずぃ",
    "ふゅ", "ひぇ", "みぇ", "りぇ", "にぇ", "びぇ", "ぴぇ", "きぇ", "ぎぇ", "ほぇ",
];

/// Number of loanword-only sound sequences in a hiragana reading.
pub fn foreign_score(reading: &str) -> usize {
    let h = kata_to_hira(reading);
    FOREIGN_SEQ.iter().map(|p| h.matches(p).count()).sum()
}

pub fn to_halfwidth_kana(s: &str) -> String {
    const MAP: &[(&str, &str)] = &[
        ("ガ", "ｶﾞ"), ("ギ", "ｷﾞ"), ("グ", "ｸﾞ"), ("ゲ", "ｹﾞ"), ("ゴ", "ｺﾞ"), ("ザ", "ｻﾞ"), ("ジ", "ｼﾞ"),
        ("ズ", "ｽﾞ"), ("ゼ", "ｾﾞ"), ("ゾ", "ｿﾞ"), ("ダ", "ﾀﾞ"), ("ヂ", "ﾁﾞ"), ("ヅ", "ﾂﾞ"), ("デ", "ﾃﾞ"),
        ("ド", "ﾄﾞ"), ("バ", "ﾊﾞ"), ("ビ", "ﾋﾞ"), ("ブ", "ﾌﾞ"), ("ベ", "ﾍﾞ"), ("ボ", "ﾎﾞ"), ("パ", "ﾊﾟ"),
        ("ピ", "ﾋﾟ"), ("プ", "ﾌﾟ"), ("ペ", "ﾍﾟ"), ("ポ", "ﾎﾟ"), ("ヴ", "ｳﾞ"),
        ("ア", "ｱ"), ("イ", "ｲ"), ("ウ", "ｳ"), ("エ", "ｴ"), ("オ", "ｵ"), ("カ", "ｶ"), ("キ", "ｷ"), ("ク", "ｸ"),
        ("ケ", "ｹ"), ("コ", "ｺ"), ("サ", "ｻ"), ("シ", "ｼ"), ("ス", "ｽ"), ("セ", "ｾ"), ("ソ", "ｿ"), ("タ", "ﾀ"),
        ("チ", "ﾁ"), ("ツ", "ﾂ"), ("テ", "ﾃ"), ("ト", "ﾄ"), ("ナ", "ﾅ"), ("ニ", "ﾆ"), ("ヌ", "ﾇ"), ("ネ", "ﾈ"),
        ("ノ", "ﾉ"), ("ハ", "ﾊ"), ("ヒ", "ﾋ"), ("フ", "ﾌ"), ("ヘ", "ﾍ"), ("ホ", "ﾎ"), ("マ", "ﾏ"), ("ミ", "ﾐ"),
        ("ム", "ﾑ"), ("メ", "ﾒ"), ("モ", "ﾓ"), ("ヤ", "ﾔ"), ("ユ", "ﾕ"), ("ヨ", "ﾖ"), ("ラ", "ﾗ"), ("リ", "ﾘ"),
        ("ル", "ﾙ"), ("レ", "ﾚ"), ("ロ", "ﾛ"), ("ワ", "ﾜ"), ("ヲ", "ｦ"), ("ン", "ﾝ"), ("ァ", "ｧ"), ("ィ", "ｨ"),
        ("ゥ", "ｩ"), ("ェ", "ｪ"), ("ォ", "ｫ"), ("ッ", "ｯ"), ("ャ", "ｬ"), ("ュ", "ｭ"), ("ョ", "ｮ"), ("ー", "ｰ"),
        ("、", "､"), ("。", "｡"), ("「", "｢"), ("」", "｣"), ("・", "･"),
    ];
    let k = hira_to_kata(s);
    k.chars()
        .map(|c| {
            let t = c.to_string();
            MAP.iter().find(|(a, _)| *a == t).map(|(_, b)| b.to_string()).unwrap_or(t)
        })
        .collect()
}

pub fn to_fullwidth_ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => '\u{3000}',
            '!'..='~' => char::from_u32(c as u32 - 0x21 + 0xFF01).unwrap(),
            _ => c,
        })
        .collect()
}
