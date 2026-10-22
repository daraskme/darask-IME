//! C ABI used by the fcitx5 addon (see addon/).
//!
//! The state is exchanged as one tab/newline separated text block:
//!   P\t<highlight 0|1>\t<text>     preedit piece
//!   C\t<kind>\t<text>              candidate
//!   K\t<cursor index or -1>        candidate cursor
//!   M\t<text>                      text to commit (at most once)

use darask_core::session::{Key, Session};
use darask_core::{Engine, Kind};
use std::ffi::{c_char, CStr, CString};
use std::path::PathBuf;

unsafe fn cstr(p: *const c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        Some(CStr::from_ptr(p).to_string_lossy().into_owned())
    }
}

#[no_mangle]
pub unsafe extern "C" fn darask_engine_new(data_dir: *const c_char, history_path: *const c_char) -> *mut Engine {
    let Some(dir) = cstr(data_dir) else { return std::ptr::null_mut() };
    let hist = cstr(history_path).map(PathBuf::from);
    match Engine::open(&PathBuf::from(dir), hist.as_deref()) {
        Ok(e) => Box::into_raw(Box::new(e)),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn darask_engine_free(e: *mut Engine) {
    if !e.is_null() {
        let mut b = Box::from_raw(e);
        b.save();
    }
}

#[no_mangle]
pub extern "C" fn darask_session_new() -> *mut Session {
    Box::into_raw(Box::new(Session::new()))
}

#[no_mangle]
pub unsafe extern "C" fn darask_session_free(s: *mut Session) {
    if !s.is_null() {
        drop(Box::from_raw(s));
    }
}

#[no_mangle]
pub unsafe extern "C" fn darask_session_reset(s: *mut Session) {
    (*s).reset();
}

/// mods: bit0 shift, bit1 ctrl, bit2 alt. Returns 1 if the key was consumed.
#[no_mangle]
pub unsafe extern "C" fn darask_session_key(e: *mut Engine, s: *mut Session, sym: u32, mods: u32) -> i32 {
    let k = Key { sym, shift: mods & 1 != 0, ctrl: mods & 2 != 0, alt: mods & 4 != 0 };
    (*s).key(&mut *e, k) as i32
}

#[no_mangle]
pub unsafe extern "C" fn darask_session_select(e: *mut Engine, s: *mut Session, index: u32) {
    (*s).select(&mut *e, index as usize);
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

fn kind_code(k: Kind) -> &'static str {
    match k {
        Kind::Conversion => "conv",
        Kind::Hiragana => "hira",
        Kind::Katakana => "kata",
        Kind::PartialKatakana => "part",
        Kind::English => "en",
        Kind::Raw => "raw",
        Kind::HalfKatakana => "half",
        Kind::FullAlpha => "full",
        Kind::Predict => "pred",
    }
}

/// Returns a malloc-like string that must be released with darask_string_free.
#[no_mangle]
pub unsafe extern "C" fn darask_session_state(e: *mut Engine, s: *mut Session) -> *mut c_char {
    let (eng, sess) = (&*e, &mut *s);
    let mut out = String::new();
    if let Some(c) = sess.take_commit() {
        out.push_str(&format!("M\t{}\n", clean(&c)));
    }
    for (t, h) in sess.preedit(eng) {
        out.push_str(&format!("P\t{}\t{}\n", h as u8, clean(&t)));
    }
    let (cands, cur) = sess.candidates();
    for c in &cands {
        out.push_str(&format!("C\t{}\t{}\n", kind_code(c.kind), clean(&c.text)));
    }
    out.push_str(&format!("K\t{}\n", cur.map_or(-1, |c| c as i64)));
    CString::new(out).unwrap_or_default().into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn darask_string_free(p: *mut c_char) {
    if !p.is_null() {
        drop(CString::from_raw(p));
    }
}
