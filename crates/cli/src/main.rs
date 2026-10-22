use darask_core::{Engine, Kind};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

fn main() {
    let dir = std::env::var("DARASK_DATA").unwrap_or_else(|_| "data/build".into());
    let engine = Engine::open(&PathBuf::from(dir), None).expect("open data (run darask-build first)");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let run = |line: &str| {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let (kana, pending) = engine.preedit(line);
        println!("▶ {}  [{}{}]", line, kana, pending);
        if std::env::var("DEBUG").is_ok() {
            for l in engine.debug_paths(line, 6) {
                println!("    {}", l);
            }
        }
        for (i, seg) in engine.convert(line).iter().enumerate() {
            let cs: Vec<String> = seg
                .candidates
                .iter()
                .take(10)
                .map(|c| {
                    let m = match c.kind {
                        Kind::English => "ᴇ",
                        Kind::Raw => "ʀ",
                        Kind::PartialKatakana => "ᴘ",
                        _ => "",
                    };
                    format!("{}{}", c.text, m)
                })
                .collect();
            println!("  [{}] {} : {}", i, seg.reading, cs.join(" / "));
        }
        let pr = engine.predict(line, 6);
        if !pr.is_empty() {
            println!("  predict: {}", pr.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" / "));
        }
    };
    if !args.is_empty() {
        for a in &args {
            run(a);
        }
        return;
    }
    let stdin = io::stdin();
    loop {
        print!("> ");
        io::stdout().flush().ok();
        let mut l = String::new();
        if stdin.lock().read_line(&mut l).unwrap_or(0) == 0 {
            break;
        }
        run(&l);
    }
}
