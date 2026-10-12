#![cfg(feature = "silero-vad")]
#![allow(clippy::expect_used)]
//! Needs `--features silero-vad`, a TTS-off sherpa-onnx (`SHERPA_ONNX_LIB_DIR`) and `silero_vad.onnx`
//! (`$PANDO_SILERO_VAD`, default `~/.cache/pando-assistant-spikes/silero_vad_v5.onnx`).

use pando_stt::vad::{SileroSegmenter, SpeechSegmenter, VadEvent};

#[test]
#[ignore = "needs the Silero model and a TTS-off sherpa-onnx build"]
fn speech_clip_between_silences_is_one_segment() {
    let model = std::env::var_os("PANDO_SILERO_VAD").map_or_else(
        || {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join(".cache/pando-assistant-spikes/silero_vad_v5.onnx")
        },
        Into::into,
    );
    let mut seg = SileroSegmenter::new(&model).expect("model");
    let wav =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/oye_pando_es.wav");
    let mut r = hound::WavReader::open(wav).expect("fixture");
    let speech: Vec<f32> = r
        .samples::<i16>()
        .map(|s| f32::from(s.expect("s")) / 32768.0)
        .collect();
    let mut pcm = vec![0.0f32; 16_000];
    pcm.extend(&speech);
    pcm.extend(vec![0.0f32; 16_000 * 2]);
    let mut ev = Vec::new();
    for c in pcm.chunks(1600) {
        seg.push(c, &mut ev);
    }
    seg.flush(&mut ev);
    let ends: Vec<_> = ev
        .iter()
        .filter(|e| matches!(e, VadEvent::End(_)))
        .collect();
    assert_eq!(ends.len(), 1, "{ev:?}");
    assert!(ev.contains(&VadEvent::Start));
    // idle CPU: 60 s of silence is far below 3 % of a core
    let t = std::time::Instant::now();
    let mut out = Vec::new();
    for _ in 0..60 {
        seg.push(&vec![0.0f32; 16_000], &mut out);
    }
    let pct = 100.0 * t.elapsed().as_secs_f64() / 60.0;
    println!("silero idle: {pct:.3} % of one core");
    assert!(out.is_empty() && pct < 3.0);
}
