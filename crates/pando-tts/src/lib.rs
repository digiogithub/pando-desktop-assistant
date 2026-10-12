//! TTS worker library (ADR 0007): the [`Tts`] trait, the sherpa-onnx Piper/Kokoro engine and the
//! `voice-proto` serving loop with sentence-pipelined synthesis and cancellation.
//!
//! This binary links espeak-ng (GPL-3) and is distributed together with its Corresponding Source.
//! It must never share memory or data structures with the app: text goes in, PCM comes out.

pub mod engine;
pub mod server;

pub use engine::{SherpaTts, Tts, TtsError, synthesize_stream};
