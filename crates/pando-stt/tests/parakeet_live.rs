#![cfg(feature = "parakeet")]
#![allow(clippy::expect_used)]
//! Needs `--features parakeet`, a TTS-off sherpa-onnx (`SHERPA_ONNX_LIB_DIR`) and the model dir
//! (`$PANDO_PARAKEET_DIR`, default `~/.cache/pando-assistant-spikes/models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8`).
//! Run: `SHERPA_ONNX_LIB_DIR=... cargo test -p pando-stt --features parakeet --test parakeet_live -- --ignored`.

use pando_stt::parakeet::Parakeet;
use pando_stt::{Stt, SttOpts};

#[test]
#[ignore = "needs the Parakeet model and a TTS-off sherpa-onnx build"]
fn spanish_fixture_is_transcribed() {
    let dir = std::env::var_os("PANDO_PARAKEET_DIR").map_or_else(
        || {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(
                ".cache/pando-assistant-spikes/models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
            )
        },
        Into::into,
    );
    let mut p = Parakeet::load(&dir, 4).expect("load");
    let wav =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/oye_pando_es.wav");
    let mut r = hound::WavReader::open(wav).expect("fixture");
    let pcm: Vec<f32> = r
        .samples::<i16>()
        .map(|s| f32::from(s.expect("s")) / 32768.0)
        .collect();
    let t = p.transcribe(&pcm, &SttOpts::default()).expect("transcribe");
    let l = t.text.to_lowercase();
    assert!(
        l.contains("pando") && l.contains("proyecto"),
        "got {:?}",
        t.text
    );
    assert!(Parakeet::load(std::path::Path::new("/nonexistent"), 1).is_err());
}
