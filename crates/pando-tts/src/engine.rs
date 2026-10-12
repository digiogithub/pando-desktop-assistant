//! TTS engines.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig, OfflineTtsVitsModelConfig,
};

#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("model load failed: {0}")]
    Load(String),
    #[error("synthesis failed: {0}")]
    Synth(String),
    #[error("unknown voice `{0}`")]
    UnknownVoice(String),
}

/// A speech synthesiser used from one thread.
pub trait Tts {
    /// Output sample rate (mono f32).
    fn sample_rate(&self) -> u32;

    /// Synthesise one sentence-sized `text`; returns its mono samples. `cancelled` is polled by the
    /// engine where it can (sherpa calls back once per generated chunk).
    fn synth_sentence(
        &mut self,
        text: &str,
        speed: f32,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<Vec<f32>, TtsError>;
}

/// Segment `text` (first chunk cut at a comma when long), synthesise each segment and hand its PCM,
/// in slices of about 200 ms, to `sink`. `sink` returns `false` to stop. Returns `Ok(false)` if
/// stopped early.
pub fn synthesize_stream(
    tts: &mut dyn Tts,
    text: &str,
    speed: f32,
    cancelled: &Arc<AtomicBool>,
    sink: &mut dyn FnMut(u32, &[f32]) -> bool,
) -> Result<bool, TtsError> {
    for seg in speech::segments(text) {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let pcm = tts.synth_sentence(&seg, speed, cancelled)?;
        if cancelled.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let rate = tts.sample_rate();
        let slice = (rate as usize / 5).max(1);
        for part in pcm.chunks(slice) {
            if cancelled.load(Ordering::SeqCst) || !sink(rate, part) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Kokoro multi-lang v1.0 speakers we map by name (others by number).
pub const KOKORO_VOICES: [(&str, i32); 4] = [
    ("ef_dora", 28),
    ("em_alex", 29),
    ("em_santa", 30),
    ("af_heart", 3),
];

fn kokoro_sid(voice: &str) -> Result<i32, TtsError> {
    if voice.is_empty() {
        return Ok(28);
    }
    if let Ok(n) = voice.parse::<i32>() {
        return Ok(n);
    }
    KOKORO_VOICES
        .iter()
        .find(|(n, _)| *n == voice)
        .map(|(_, sid)| *sid)
        .ok_or_else(|| TtsError::UnknownVoice(voice.to_string()))
}

/// sherpa-onnx `OfflineTts` for Piper (VITS) and Kokoro model directories.
pub struct SherpaTts {
    tts: OfflineTts,
    sid: i32,
    rate: u32,
}

impl std::fmt::Debug for SherpaTts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SherpaTts(sid {}, {} Hz)", self.sid, self.rate)
    }
}

fn first_onnx(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "onnx"))
        .collect();
    found.sort();
    found.into_iter().next()
}

fn s(p: PathBuf) -> Option<String> {
    Some(p.to_string_lossy().into_owned())
}

impl SherpaTts {
    /// Load a model directory: a Kokoro one (has `voices.bin`) or a Piper one (`*.onnx`, `tokens.txt`,
    /// `espeak-ng-data/`). `voice` selects the Kokoro speaker (name or number); Piper ignores it.
    pub fn load(dir: &Path, voice: &str, threads: i32) -> Result<Self, TtsError> {
        let threads = threads.max(1);
        let (model, sid) = if dir.join("voices.bin").is_file() {
            let model = if dir.join("model.onnx").is_file() {
                dir.join("model.onnx")
            } else {
                dir.join("model.int8.onnx")
            };
            let sid = kokoro_sid(voice)?;
            let lex = |f: &str| dir.join(f).to_string_lossy().into_owned();
            let lexicons: Vec<String> = ["lexicon-us-en.txt", "lexicon-zh.txt"]
                .iter()
                .filter(|f| dir.join(f).is_file())
                .map(|f| lex(f))
                .collect();
            let cfg = OfflineTtsConfig {
                model: OfflineTtsModelConfig {
                    kokoro: OfflineTtsKokoroModelConfig {
                        model: s(model.clone()),
                        voices: s(dir.join("voices.bin")),
                        tokens: s(dir.join("tokens.txt")),
                        data_dir: s(dir.join("espeak-ng-data")),
                        lexicon: (!lexicons.is_empty()).then(|| lexicons.join(",")),
                        dict_dir: dir.join("dict").is_dir().then(|| lex("dict")),
                        lang: Some("es".into()),
                        ..Default::default()
                    },
                    num_threads: threads,
                    provider: Some("cpu".into()),
                    ..Default::default()
                },
                max_num_sentences: 1,
                ..Default::default()
            };
            (OfflineTts::create(&cfg), sid)
        } else {
            let model = first_onnx(dir)
                .ok_or_else(|| TtsError::Load(format!("no .onnx model in {}", dir.display())))?;
            let cfg = OfflineTtsConfig {
                model: OfflineTtsModelConfig {
                    vits: OfflineTtsVitsModelConfig {
                        model: s(model),
                        tokens: s(dir.join("tokens.txt")),
                        data_dir: s(dir.join("espeak-ng-data")),
                        ..Default::default()
                    },
                    num_threads: threads,
                    provider: Some("cpu".into()),
                    ..Default::default()
                },
                max_num_sentences: 1,
                ..Default::default()
            };
            (OfflineTts::create(&cfg), voice.parse::<i32>().unwrap_or(0))
        };
        let tts = model.ok_or_else(|| {
            TtsError::Load(format!("sherpa-onnx could not load {}", dir.display()))
        })?;
        let rate = tts.sample_rate().max(1) as u32;
        Ok(Self { tts, sid, rate })
    }

    pub fn num_speakers(&self) -> i32 {
        self.tts.num_speakers()
    }
}

impl Tts for SherpaTts {
    fn sample_rate(&self) -> u32 {
        self.rate
    }

    fn synth_sentence(
        &mut self,
        text: &str,
        speed: f32,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<Vec<f32>, TtsError> {
        let cfg = GenerationConfig {
            sid: self.sid,
            speed: if speed > 0.0 { speed } else { 1.0 },
            ..Default::default()
        };
        let flag = cancelled.clone();
        let audio = self
            .tts
            .generate_with_config(
                text,
                &cfg,
                Some(move |_s: &[f32], _p: f32| !flag.load(Ordering::SeqCst)),
            )
            .ok_or_else(|| TtsError::Synth("the engine returned no audio".into()))?;
        Ok(audio.samples().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake engine: one "sample" per character, 10 Hz.
    struct Fake;
    impl Tts for Fake {
        fn sample_rate(&self) -> u32 {
            10
        }
        fn synth_sentence(
            &mut self,
            text: &str,
            _speed: f32,
            _c: &Arc<AtomicBool>,
        ) -> Result<Vec<f32>, TtsError> {
            Ok(vec![0.5; text.chars().count()])
        }
    }

    #[test]
    fn streams_every_sentence_in_order() {
        let mut got = Vec::new();
        let c = Arc::new(AtomicBool::new(false));
        let done = synthesize_stream(&mut Fake, "Hola. Adiós.", 1.0, &c, &mut |r, p| {
            got.push((r, p.len()));
            true
        })
        .expect("ok");
        assert!(done);
        // 5 + 6 samples, 10 Hz -> 2-sample slices
        assert_eq!(got.iter().map(|g| g.1).sum::<usize>(), 11);
        assert!(got.iter().all(|g| g.0 == 10 && g.1 <= 2));
    }

    #[test]
    fn sink_can_stop_the_stream() {
        let c = Arc::new(AtomicBool::new(false));
        let mut n = 0;
        let done = synthesize_stream(&mut Fake, "Uno. Dos. Tres.", 1.0, &c, &mut |_, _| {
            n += 1;
            n < 2
        })
        .expect("ok");
        assert!(!done);
        assert_eq!(n, 2);
    }

    #[test]
    fn cancel_flag_stops_before_next_sentence() {
        struct Cancelling(Arc<AtomicBool>);
        impl Tts for Cancelling {
            fn sample_rate(&self) -> u32 {
                10
            }
            fn synth_sentence(
                &mut self,
                _t: &str,
                _s: f32,
                _c: &Arc<AtomicBool>,
            ) -> Result<Vec<f32>, TtsError> {
                self.0.store(true, Ordering::SeqCst);
                Ok(vec![0.1; 4])
            }
        }
        let c = Arc::new(AtomicBool::new(false));
        let mut chunks = 0;
        let done = synthesize_stream(
            &mut Cancelling(c.clone()),
            "Uno. Dos.",
            1.0,
            &c,
            &mut |_, _| {
                chunks += 1;
                true
            },
        )
        .expect("ok");
        assert!(!done);
        assert_eq!(chunks, 0, "cancelled audio is never emitted");
    }

    #[test]
    fn kokoro_voice_names() {
        assert_eq!(kokoro_sid("ef_dora").ok(), Some(28));
        assert_eq!(kokoro_sid("").ok(), Some(28));
        assert_eq!(kokoro_sid("7").ok(), Some(7));
        assert!(kokoro_sid("nope").is_err());
    }

    #[test]
    fn missing_model_dir_is_a_load_error() {
        assert!(matches!(
            SherpaTts::load(Path::new("/nonexistent/model"), "", 1),
            Err(TtsError::Load(_))
        ));
    }
}
