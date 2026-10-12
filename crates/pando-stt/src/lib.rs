//! STT worker library (ADR 0007, ADR 0008): the [`Stt`] trait, the Whistle engine
//! ([`whistle`]), pause-aware chunking ([`chunking`]) and the stdio protocol loop ([`server`]).
//!
//! This crate must never link a prebuilt sherpa-onnx (it embeds espeak-ng, GPL-3).

pub mod chunking;
pub mod engine;
pub mod handsfree;
pub mod keywords;
pub mod live;
#[cfg(feature = "parakeet")]
pub mod parakeet;
pub mod server;
pub mod stt;
pub mod vad;
pub mod wake;
pub mod whistle;

pub use stt::{Partial, Stt, SttError, SttOpts, Transcript};
pub use whistle::Whistle;
