use darask_core::session::{sym, Key, Session};
use darask_core::Engine;
use std::path::PathBuf;

fn engine() -> Option<Engine> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/build");
    dir.join("dict.fst").exists().then(|| Engine::open(&dir, None).unwrap())
}

fn type_str(s: &mut Session, e: &mut Engine, text: &str) {
    for c in text.chars() {
        s.key(e, Key { sym: c as u32, ..Default::default() });
    }
}

fn press(s: &mut Session, e: &mut Engine, sym: u32) -> bool {
    s.key(e, Key { sym, ..Default::default() })
}

#[test]
fn extension_is_offered_while_typing() {
    let Some(mut e) = engine() else { return };
    let mut s = Session::new();
    type_str(&mut s, &mut e, "ekusutensyon");
    let (cands, _) = s.candidates();
    let texts: Vec<&str> = cands.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts[0], "エクステンション");
    assert!(texts.iter().take(4).any(|t| *t == "extension"), "{:?}", texts);
}

#[test]
fn convert_and_commit_sentence() {
    let Some(mut e) = engine() else { return };
    let mut s = Session::new();
    type_str(&mut s, &mut e, "kyouhaiitenkidesune");
    press(&mut s, &mut e, sym::SPACE);
    press(&mut s, &mut e, sym::RETURN);
    assert_eq!(s.take_commit().unwrap(), "今日はいい天気ですね");
    assert!(!s.is_active());
}

#[test]
fn partial_katakana_and_learning() {
    let Some(mut e) = engine() else { return };
    let mut s = Session::new();
    type_str(&mut s, &mut e, "kifewonomu");
    press(&mut s, &mut e, sym::SPACE);
    press(&mut s, &mut e, sym::RETURN);
    assert_eq!(s.take_commit().unwrap(), "キフェを飲む");
}

#[test]
fn enter_commits_hiragana() {
    let Some(mut e) = engine() else { return };
    let mut s = Session::new();
    type_str(&mut s, &mut e, "nihongo");
    press(&mut s, &mut e, sym::RETURN);
    assert_eq!(s.take_commit().unwrap(), "にほんご");
}

fn press_shift(s: &mut Session, e: &mut Engine, sym: u32) {
    s.key(e, Key { sym, shift: true, ..Default::default() });
}

#[test]
fn mixed_english_japanese() {
    let Some(mut e) = engine() else { return };
    let mut s = Session::new();
    type_str(&mut s, &mut e, "dockerwoinsutorusuru");
    press(&mut s, &mut e, sym::SPACE);
    press(&mut s, &mut e, sym::RETURN);
    assert_eq!(s.take_commit().unwrap(), "dockerをインストールする");
}

#[test]
fn resize_segment() {
    let Some(mut e) = engine() else { return };
    let mut s = Session::new();
    type_str(&mut s, &mut e, "kyouhaiitenkidesune");
    press(&mut s, &mut e, sym::SPACE);
    press_shift(&mut s, &mut e, sym::LEFT);
    let (cands, _) = s.candidates();
    assert!(cands[0].text.starts_with("今日"), "{:?}", cands[0]);
    press(&mut s, &mut e, sym::RETURN);
    assert!(s.take_commit().unwrap().contains("天気"));
}

#[test]
fn learns_unknown_words() {
    let Some(mut e) = engine() else { return };
    let mut s = Session::new();
    // "ぬこ" has no dictionary entry for 猫缶ちゃん; teach it
    e.learn("ぬこかんちゃん", "ヌコ缶ちゃん");
    type_str(&mut s, &mut e, "nukokanchan");
    press(&mut s, &mut e, sym::SPACE);
    press(&mut s, &mut e, sym::RETURN);
    assert_eq!(s.take_commit().unwrap(), "ヌコ缶ちゃん");
}

#[test]
fn english_edge_inside_romaji_chunk() {
    let Some(mut e) = engine() else { return };
    let mut s = Session::new();
    type_str(&mut s, &mut e, "kyouhapushisita");
    press(&mut s, &mut e, sym::SPACE);
    press(&mut s, &mut e, sym::RETURN);
    assert!(s.take_commit().unwrap().starts_with("今日はpush"));
}

#[test]
fn english_prefix_prediction() {
    let Some(mut e) = engine() else { return };
    let mut s = Session::new();
    type_str(&mut s, &mut e, "ekusuten");
    let (cands, _) = s.candidates();
    let texts: Vec<&str> = cands.iter().map(|c| c.text.as_str()).collect();
    assert!(texts.contains(&"extend") || texts.contains(&"extent") || texts.contains(&"extension"), "{:?}", texts);
}
