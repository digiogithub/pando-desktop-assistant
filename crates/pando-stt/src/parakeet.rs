//! Parakeet-TDT-0.6b-v3 (int8) through sherpa-onnx `OfflineRecognizer` (feature `parakeet`).
//!
//! The sherpa-onnx used here MUST be the TTS-off source build (`scripts/build-sherpa-stt.sh`),
//! never the prebuilt archives (ADR 0007).
//!
//! Parakeet v3 is multilingual with automatic language detection: `SttOpts::lang` and
//! `keywords` are accepted but ignored. Latency grows linearly with audio length, so it is the
//! engine for utterances longer than Whistle's 5 s fast path (ADR 0008).

use std::path::Path;

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

use crate::stt::{Stt, SttError, SttOpts, Transcript};

/// Files expected inside the model directory.
pub const MODEL_FILES: [&str; 4] = [
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "joiner.int8.onnx",
    "tokens.txt",
];

/// Parakeet engine.
pub struct Parakeet {
    rec: OfflineRecognizer,
}

impl std::fmt::Debug for Parakeet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Parakeet")
    }
}

impl Parakeet {
    /// Load from a directory holding [`MODEL_FILES`].
    pub fn load(dir: &Path, threads: i32) -> Result<Self, SttError> {
        for f in MODEL_FILES {
            if !dir.join(f).is_file() {
                return Err(SttError::Load(format!("{} is missing {f}", dir.display())));
            }
        }
        let p = |f: &str| Some(dir.join(f).to_string_lossy().into_owned());
        let mut cfg = OfflineRecognizerConfig::default();
        cfg.model_config.transducer = OfflineTransducerModelConfig {
            encoder: p("encoder.int8.onnx"),
            decoder: p("decoder.int8.onnx"),
            joiner: p("joiner.int8.onnx"),
        };
        cfg.model_config.tokens = p("tokens.txt");
        cfg.model_config.model_type = Some("nemo_transducer".into());
        cfg.model_config.num_threads = threads.max(1);
        let rec = OfflineRecognizer::create(&cfg)
            .ok_or_else(|| SttError::Load("sherpa-onnx could not create the recognizer".into()))?;
        Ok(Self { rec })
    }
}

impl Stt for Parakeet {
    fn name(&self) -> &'static str {
        "parakeet"
    }

    fn transcribe(&mut self, pcm: &[f32], _opts: &SttOpts) -> Result<Transcript, SttError> {
        let stream = self.rec.create_stream();
        stream.accept_waveform(voice_proto::STT_SAMPLE_RATE as i32, pcm);
        self.rec.decode(&stream);
        let r = stream
            .get_result()
            .ok_or_else(|| SttError::Engine("no result from the recognizer".into()))?;
        Ok(Transcript {
            text: r.text.trim().to_string(),
            lang: String::new(),
            words: Vec::new(),
            ttft_ms: None,
        })
    }
}
