//! Capture and playback for the assistant (see `README.md`).
//!
//! Real-time rule: audio callbacks never allocate or lock. Everything that does (resampling,
//! callbacks into user code, stream rebuilds) runs on the supervisor threads.

#[cfg(feature = "echo-cancel")]
pub mod aec;
pub mod bargein;
pub mod capture;
pub mod device;
pub mod level;
pub mod playback;
pub mod preroll;
pub mod resample;

#[cfg(feature = "echo-cancel")]
pub use aec::{EchoCancel, EchoHandle, EchoProcessor, SonoraAec, looks_like_headset};
pub use capture::{Capture, CaptureConfig, CaptureEvent};
pub use device::{DeviceInfo, list_input_devices, list_output_devices};
pub use level::{LevelMeter, rms};
pub use playback::{Playback, PlaybackConfig, PlaybackEvent, ReferenceTap};
pub use preroll::PreRoll;
pub use resample::StreamResampler;

/// Sample rate delivered by [`Capture`] and consumed by the STT worker.
pub const CAPTURE_RATE: u32 = 16_000;

/// Audio errors.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("audio host `{0}` is not available")]
    HostUnavailable(String),
    #[error("no audio device available: {0}")]
    NoDevice(String),
    #[error("unsupported sample format {0}")]
    UnsupportedFormat(String),
    #[error("audio backend error: {0}")]
    Backend(String),
    #[error("resampler error: {0}")]
    Resample(String),
}
