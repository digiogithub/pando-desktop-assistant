#![allow(clippy::expect_used)]
//! Needs a Piper/Kokoro model directory and links the real sherpa-onnx.
//! `PANDO_PIPER_DIR` (default `~/.cache/pando-assistant-spikes/vits-piper-es_ES-davefx-medium`),
//! `PANDO_KOKORO_DIR` (default `.../kokoro-multi-lang-v1_0`, optional).
//! Run: `cargo test -p pando-tts --test piper_live -- --ignored --nocapture --test-threads=1`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use pando_tts::{SherpaTts, Tts, synthesize_stream};

fn dir(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var).map_or_else(
        || {
            PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join(".cache/pando-assistant-spikes")
                .join(default)
        },
        Into::into,
    )
}

fn piper() -> SherpaTts {
    SherpaTts::load(
        &dir("PANDO_PIPER_DIR", "vits-piper-es_ES-davefx-medium"),
        "",
        2,
    )
    .expect("load piper")
}

const TEN_WORDS: &str = "La tarea de migración auth ha terminado sin errores hoy.";

#[test]
#[ignore = "needs the Piper es_ES-davefx-medium model directory"]
fn piper_first_chunk_under_200_ms() {
    let mut tts = piper();
    assert!(tts.sample_rate() >= 16_000);
    let cancel = Arc::new(AtomicBool::new(false));
    let mut run = |label: &str| -> (u128, usize) {
        let t = Instant::now();
        let mut first = None;
        let mut samples = 0;
        let done = synthesize_stream(&mut tts, TEN_WORDS, 1.0, &cancel, &mut |_, pcm| {
            first.get_or_insert(t.elapsed());
            samples += pcm.len();
            true
        })
        .expect("synth");
        assert!(done);
        let ms = first.expect("audio").as_millis();
        println!("{label}: first chunk {ms} ms, {samples} samples");
        (ms, samples)
    };
    let (cold, n) = run("cold");
    assert!(n > 10_000, "about 3 s of audio expected, got {n} samples");
    let (warm, _) = run("warm");
    assert!(warm < 200, "warm first chunk {warm} ms");
    assert!(cold < 600, "cold first chunk {cold} ms");
}

#[test]
#[ignore = "needs the Piper model directory"]
fn long_text_streams_sentence_by_sentence() {
    let mut tts = piper();
    let cancel = Arc::new(AtomicBool::new(false));
    let text = "He revisado el proyecto. La migración terminó y los tests pasan. ¿Quieres que abra la pull request?";
    let t = Instant::now();
    let mut stamps = Vec::new();
    synthesize_stream(&mut tts, text, 1.0, &cancel, &mut |_, _| {
        stamps.push(t.elapsed().as_millis());
        true
    })
    .expect("synth");
    assert!(stamps.len() >= 3);
    println!("chunk arrival times (ms): {stamps:?}");
}

#[test]
#[ignore = "needs the Kokoro model directory"]
fn kokoro_loads_and_speaks() {
    let mut tts = SherpaTts::load(
        &dir("PANDO_KOKORO_DIR", "kokoro-multi-lang-v1_0"),
        "ef_dora",
        4,
    )
    .expect("load kokoro");
    let cancel = Arc::new(AtomicBool::new(false));
    let mut n = 0;
    synthesize_stream(&mut tts, "Hola, ¿qué tal?", 1.0, &cancel, &mut |_, p| {
        n += p.len();
        true
    })
    .expect("synth");
    assert!(n > 5000);
}

#[test]
#[ignore = "needs the Piper model directory"]
fn cancel_stops_mid_stream() {
    let mut tts = piper();
    let cancel = Arc::new(AtomicBool::new(false));
    let c2 = cancel.clone();
    let mut chunks = 0;
    let done = synthesize_stream(
        &mut tts,
        "Primera frase de prueba. Segunda frase de prueba. Tercera frase de prueba.",
        1.0,
        &cancel,
        &mut |_, _| {
            chunks += 1;
            c2.store(true, Ordering::SeqCst);
            true
        },
    )
    .expect("synth");
    assert!(!done);
    assert_eq!(chunks, 1);
}
