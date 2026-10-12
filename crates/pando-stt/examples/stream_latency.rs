#![allow(clippy::expect_used, clippy::print_stdout)]
//! Profiles the push-to-talk streaming path: feeds a WAV in 0.75 s chunks (like the app) and
//! times each `stream_process` and the final `stream_stop` (PDA-T-0095).
//! `cargo run --release -p pando-stt --example stream_latency -- clip.wav [chunk_samples] [rounds]`
use pando_stt::{Stt, SttOpts, Whistle};
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut r = hound::WavReader::open(&a[1]).expect("wav");
    let spec = r.spec();
    let scale = (1u32 << (spec.bits_per_sample - 1)) as f32;
    let pcm: Vec<f32> = r
        .samples::<i32>()
        .map(|s| s.expect("s") as f32 / scale)
        .collect();
    let chunk: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(12_000);
    let rounds: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(5);
    let model = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/needle/models/whistle.cact");
    let mut w = Whistle::load_file(&model).expect("model");
    let opts = SttOpts {
        lang: Some("es".into()),
        keywords: vec!["Pando".into(), "Bitácora".into()],
        word_ts: false,
    };
    for round in 0..rounds {
        let mut proc = Vec::new();
        for c in pcm.chunks(chunk) {
            let t = Instant::now();
            let p = w.stream_process(c, &opts).expect("process");
            proc.push((t.elapsed().as_millis(), c.len(), p.text));
        }
        let t = Instant::now();
        let fin = w.stream_stop().expect("stop");
        println!(
            "round {round}: {:.2}s audio, process ms {:?}, stop {} ms, tail {:?}",
            pcm.len() as f32 / 16000.0,
            proc.iter().map(|p| p.0).collect::<Vec<_>>(),
            t.elapsed().as_millis(),
            fin.text
        );
    }
    let t = Instant::now();
    let full = w.transcribe(&pcm, &opts).expect("full");
    println!(
        "one-shot transcribe: {} ms: {:?}",
        t.elapsed().as_millis(),
        full.text
    );
}
