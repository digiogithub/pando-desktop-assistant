#![allow(clippy::expect_used)]
//! Synthesizes a sentence to a 16-bit mono WAV, for hand-driven voice checks.
//! `cargo run -p pando-tts --example say -- <model-dir> "<text>" <out.wav>` (model rate, mono).

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use pando_tts::{SherpaTts, Tts, synthesize_stream};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, text, out) = (
        args.first().expect("model dir"),
        args.get(1).expect("text"),
        args.get(2).expect("out.wav"),
    );
    let mut tts = SherpaTts::load(std::path::Path::new(dir), "", 2).expect("load model");
    let rate = tts.sample_rate();
    let mut pcm = Vec::new();
    let cancel = Arc::new(AtomicBool::new(false));
    synthesize_stream(&mut tts, text, 1.0, &cancel, &mut |_, chunk| {
        pcm.extend_from_slice(chunk);
        true
    })
    .expect("synthesize");
    let data: Vec<u8> = pcm
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    std::fs::write(out, wav).expect("write wav");
}
