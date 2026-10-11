//! Decision layer (System One): "the model classifies, the code decides".
//!
//! No UI and no network dependency: the HTTP round trip goes through
//! [`transport::DecisionTransport`], implemented by the app. See
//! `docs/03-decision-layer.md` and spec `PDA-SP-0008`.
//!
//! - [`protocol`]: `/v1/systemone` wire types, validation and answer checks.
//! - [`transport`]: the transport trait and [`transport::SystemOneClient`].
//! - [`error`]: typed, fail-open errors.

pub mod error;
pub mod protocol;
pub mod transport;

pub use error::DecisionError;
pub use transport::{ClientOptions, DecisionTransport, SystemOneClient};
