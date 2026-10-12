//! Engine-neutral STT types shared by Whistle and the optional Parakeet fallback.

use voice_proto::Word;

/// Per-call options.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SttOpts {
    /// Forced language code (`es`, `en`, ...); `None` lets the engine detect it.
    pub lang: Option<String>,
    /// Biasing phrases (project names, agent ids, "Pando").
    pub keywords: Vec<String>,
    /// Ask for per-word timestamps.
    pub word_ts: bool,
}

/// A finished transcription.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transcript {
    pub text: String,
    pub lang: String,
    pub words: Vec<Word>,
    pub ttft_ms: Option<f32>,
}

/// A live-stream update: committed text of this call plus the unconfirmed tail.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Partial {
    pub text: String,
    pub pending: String,
    pub words: Vec<Word>,
}

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("model load failed: {0}")]
    Load(String),
    #[error("engine error: {0}")]
    Engine(String),
    #[error("engine returned malformed JSON: {0}")]
    Json(String),
    #[error("a speech model is already loaded in this process")]
    AlreadyLoaded,
    #[error("{0} is not supported by this engine")]
    Unsupported(&'static str),
    #[error("invalid argument: {0}")]
    Invalid(String),
}

/// Common interface of the speech engines. Implementations are used from a single thread.
pub trait Stt {
    /// Engine identifier (`whistle`, `parakeet`).
    fn name(&self) -> &'static str;

    /// Longest audio (seconds) the engine handles at full speed in one call, if any.
    fn preferred_max_seconds(&self) -> Option<f32> {
        None
    }

    /// Transcribe 16 kHz mono f32 PCM.
    fn transcribe(&mut self, pcm: &[f32], opts: &SttOpts) -> Result<Transcript, SttError>;

    /// Feed ~1 s of a live stream.
    fn stream_process(&mut self, _pcm: &[f32], _opts: &SttOpts) -> Result<Partial, SttError> {
        Err(SttError::Unsupported("streaming"))
    }

    /// End the live stream and return the final transcript.
    fn stream_stop(&mut self) -> Result<Transcript, SttError> {
        Err(SttError::Unsupported("streaming"))
    }
}
