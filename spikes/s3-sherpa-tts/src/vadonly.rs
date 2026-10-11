//! Does a binary that only uses sherpa-onnx's VAD still pull espeak-ng in?
//! (GPL check for ADR 0007; inspect with `nm | grep espeak`.)
use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};
fn main() {
    let model = std::env::args().nth(1).unwrap_or_default();
    let cfg = VadModelConfig {
        silero_vad: SileroVadModelConfig {
            model: Some(model),
            threshold: 0.5,
            window_size: 512,
            ..Default::default()
        },
        sample_rate: 16_000,
        num_threads: 1,
        ..Default::default()
    };
    println!(
        "vad created: {}",
        VoiceActivityDetector::create(&cfg, 10.0).is_some()
    );
}
