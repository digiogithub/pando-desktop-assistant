//! Model manifest and verified downloader (PDA-T-0148, `PDA-SP-0012`).
//!
//! - [`Manifest`]: the embedded `models.toml` (id, pinned URL, SHA-256, size, licence, attribution).
//! - [`ModelStore`]: the models directory (`PANDO_ASSISTANT_MODELS_DIR` or the per-user data dir),
//!   `list`, `verify`, `import` from an offline folder, `path`.
//! - [`download`]: resumable `.part` download through a [`Transport`], SHA-256 check, atomic rename.
//!   A wrong hash deletes the file and fails with [`ModelError::HashMismatch`].
//!
//! No UI and no audio: used by the app and by both workers.

pub mod download;
pub mod manifest;
pub mod store;
mod unpack;

#[cfg(feature = "http")]
pub use download::HttpTransport;
pub use download::{Cancel, Progress, Transport, download};
pub use manifest::{Manifest, ModelEntry};
pub use store::{ModelState, ModelStore};

/// Environment variable that overrides the models directory (air-gapped installs, tests).
pub const MODELS_DIR_ENV: &str = "PANDO_ASSISTANT_MODELS_DIR";

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("unknown model `{0}`")]
    Unknown(String),
    #[error("SHA-256 mismatch for `{id}`: expected {expected}, got {got}; the file was deleted")]
    HashMismatch {
        id: String,
        expected: String,
        got: String,
    },
    #[error("`{id}` has the wrong size: expected {expected} bytes, got {got}")]
    SizeMismatch { id: String, expected: u64, got: u64 },
    #[error("download of `{id}` failed: {message}")]
    Download { id: String, message: String },
    #[error("download of `{0}` was cancelled")]
    Cancelled(String),
    #[error("invalid manifest: {0}")]
    Manifest(String),
    #[error("unsafe path in archive: {0}")]
    UnsafePath(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}
