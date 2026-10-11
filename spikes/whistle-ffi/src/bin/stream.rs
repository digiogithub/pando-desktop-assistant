//! `stream <whistle.cact> <clip.wav> [chunk_s]`: feeds the clip to the live transcriber in
//! chunks and prints per-call wall time, then the final stop (the latency after key-up).
use std::time::Instant;
use whistle_spike::{Whistle, read_wav};

fn main() -> Result<(), String> {
    let a: Vec<String> = std::env::args().collect();
    let mut w = Whistle::load(&std::fs::read(&a[1]).map_err(|e| e.to_string())?)?;
    let pcm = read_wav(&a[2])?;
    let chunk = (a.get(3).map_or(1.0, |s| s.parse::<f64>().unwrap_or(1.0)) * 16000.0) as usize;
    let mut calls = Vec::new();
    for c in pcm.chunks(chunk) {
        let t = Instant::now();
        let j = w.stream_process(c, Some("es"), None)?;
        calls.push(t.elapsed().as_secs_f64() * 1e3);
        let v: serde_json::Value = serde_json::from_str(&j).unwrap_or_default();
        eprintln!(
            "{:6.1} ms  +{:?}  pending={:?}",
            calls[calls.len() - 1],
            v["text"],
            v["pending"]
        );
    }
    let t = Instant::now();
    let j = w.stream_stop()?;
    let stop = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "{}",
        serde_json::json!({"clip_s": pcm.len() as f64 / 16000.0, "calls_ms": calls, "stop_ms": stop, "stop": j})
    );
    Ok(())
}
