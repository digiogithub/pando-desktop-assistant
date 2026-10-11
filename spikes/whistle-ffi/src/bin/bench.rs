//! `bench <whistle.cact> <list.tsv> [--lang es] [--keywords file]` -> JSONL on stdout.
//! list.tsv: `id<TAB>wav_path` per line. Prints id, transcript JSON, wall ms and audio seconds.
use std::io::{BufRead, Write};
use std::time::Instant;
use whistle_spike::{Whistle, read_wav};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let cact = std::fs::read(&args[1]).map_err(|e| e.to_string())?;
    let list = std::fs::File::open(&args[2]).map_err(|e| e.to_string())?;
    let mut lang: Option<String> = None;
    let mut keywords: Option<String> = None;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--lang" => {
                lang = Some(args[i + 1].clone());
                i += 2;
            }
            "--keywords" => {
                keywords = Some(
                    std::fs::read_to_string(&args[i + 1])
                        .map_err(|e| e.to_string())?
                        .trim()
                        .to_string(),
                );
                i += 2;
            }
            other => return Err(format!("unknown arg {other}")),
        }
    }
    let t = Instant::now();
    let mut w = Whistle::load(&cact)?;
    eprintln!("load_ms={:.1}", t.elapsed().as_secs_f64() * 1e3);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in std::io::BufReader::new(list).lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Some((id, path)) = line.split_once('\t') else {
            continue;
        };
        let pcm = read_wav(path)?;
        let t = Instant::now();
        let json = w.transcribe(&pcm, lang.as_deref(), keywords.as_deref(), false)?;
        let ms = t.elapsed().as_secs_f64() * 1e3;
        let v = serde_json::json!({"id": id, "ms": ms, "audio_s": pcm.len() as f64 / 16000.0,
            "result": serde_json::from_str::<serde_json::Value>(&json).unwrap_or(serde_json::Value::String(json))});
        writeln!(out, "{v}").map_err(|e| e.to_string())?;
    }
    Ok(())
}
