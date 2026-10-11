//! S2 host: `stt-host <sidecar-exe> <clip.wav> <rounds> -- <sidecar args...>`.
//! Spawns the sidecar, sends the clip `rounds` times and reports round-trip, engine time and
//! IPC overhead (round-trip - engine_ms) as P50/P95/max in ms. ABI-agnostic: only bytes cross.
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Instant;

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let pcm = whistle_spike::read_wav(&args[2])?;
    let rounds: usize = args[3].parse().map_err(|_| "rounds")?;
    let sidecar_args = args
        .iter()
        .skip_while(|a| *a != "--")
        .skip(1)
        .cloned()
        .collect::<Vec<_>>();
    let t = Instant::now();
    let mut child = Command::new(&args[1])
        .args(&sidecar_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("spawn: {e}"))?;
    let mut tx = child.stdin.take().ok_or("stdin")?;
    let mut rx = child.stdout.take().ok_or("stdout")?;
    let mut frame = Vec::with_capacity(4 + pcm.len() * 4);
    frame.extend((pcm.len() as u32).to_le_bytes());
    for s in &pcm {
        frame.extend(s.to_le_bytes());
    }
    let (mut rtt, mut eng, mut ovh) = (vec![], vec![], vec![]);
    let mut text = String::new();
    for i in 0..rounds {
        let t0 = Instant::now();
        tx.write_all(&frame).map_err(|e| e.to_string())?;
        tx.flush().map_err(|e| e.to_string())?;
        let mut len = [0u8; 4];
        rx.read_exact(&mut len).map_err(|e| e.to_string())?;
        let mut body = vec![0u8; u32::from_le_bytes(len) as usize];
        rx.read_exact(&mut body).map_err(|e| e.to_string())?;
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let v: serde_json::Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        let e = v["engine_ms"].as_f64().unwrap_or(0.0);
        if i == 0 {
            eprintln!(
                "first round (incl. model load) {ms:.1} ms; startup+first {:.1} ms",
                t.elapsed().as_secs_f64() * 1e3
            );
            text = v["text"].as_str().unwrap_or("").to_string();
            continue; // warm-up round excluded
        }
        rtt.push(ms);
        eng.push(e);
        ovh.push(ms - e);
    }
    tx.write_all(&0u32.to_le_bytes())
        .map_err(|e| e.to_string())?;
    drop(tx);
    let _ = child.wait();
    println!(
        "{}",
        serde_json::json!({
            "clip_s": pcm.len() as f64 / 16000.0, "bytes": frame.len(), "rounds": rtt.len(), "text": text,
            "rtt_p50": pct(&mut rtt.clone(), 0.5), "rtt_p95": pct(&mut rtt.clone(), 0.95),
            "engine_p50": pct(&mut eng.clone(), 0.5),
            "ipc_overhead_p50": pct(&mut ovh.clone(), 0.5), "ipc_overhead_p95": pct(&mut ovh.clone(), 0.95),
            "ipc_overhead_max": pct(&mut ovh, 1.0),
        })
    );
    Ok(())
}
