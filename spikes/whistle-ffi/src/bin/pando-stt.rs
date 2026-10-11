//! S2 sidecar: `pando-stt <whistle.cact> [--lang es] [--keywords file] [--echo]`.
//! stdin frames:  u32 LE n_samples, then n_samples f32 LE (16 kHz mono PCM). n_samples = 0 ends.
//! stdout frames: u32 LE len, then `len` bytes of UTF-8 JSON = engine JSON + "engine_ms".
//! `--echo` skips the engine (pure IPC cost). Logs go to stderr only.
use std::io::{Read, Write};
use std::time::Instant;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let echo = args.iter().any(|a| a == "--echo");
    let lang = args
        .iter()
        .position(|a| a == "--lang")
        .map(|i| args[i + 1].clone());
    let kw = match args.iter().position(|a| a == "--keywords") {
        Some(i) => Some(
            std::fs::read_to_string(&args[i + 1])
                .map_err(|e| e.to_string())?
                .trim()
                .to_string(),
        ),
        None => None,
    };
    let mut engine = if echo {
        None
    } else {
        let cact = std::fs::read(&args[1]).map_err(|e| format!("{}: {e}", args[1]))?;
        Some(whistle_spike::Whistle::load(&cact)?)
    };
    let stdin = std::io::stdin();
    let mut inp = stdin.lock();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut buf: Vec<u8> = Vec::new();
    let mut pcm: Vec<f32> = Vec::new();
    loop {
        let mut len = [0u8; 4];
        if inp.read_exact(&mut len).is_err() {
            break;
        }
        let n = u32::from_le_bytes(len) as usize;
        if n == 0 {
            break;
        }
        buf.resize(n * 4, 0);
        inp.read_exact(&mut buf).map_err(|e| e.to_string())?;
        pcm.clear();
        pcm.extend(
            buf.chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])),
        );
        let t = Instant::now();
        let json = match engine.as_mut() {
            Some(w) => w.transcribe(&pcm, lang.as_deref(), kw.as_deref(), false)?,
            None => format!("{{\"text\":\"\",\"samples\":{}}}", pcm.len()),
        };
        let ms = t.elapsed().as_secs_f64() * 1e3;
        let mut v: serde_json::Value =
            serde_json::from_str(&json).unwrap_or(serde_json::json!({"raw": json}));
        v["engine_ms"] = serde_json::json!(ms);
        let body = v.to_string();
        out.write_all(&(body.len() as u32).to_le_bytes())
            .map_err(|e| e.to_string())?;
        out.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}
