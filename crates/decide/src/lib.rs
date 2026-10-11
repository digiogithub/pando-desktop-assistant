//! Decision layer (System One): "the model classifies, the code decides".
//!
//! No UI and no network dependency: the HTTP round trip goes through
//! [`transport::DecisionTransport`], implemented by the app. See
//! `docs/03-decision-layer.md` and spec `PDA-SP-0008`.
//!
//! - [`protocol`]: `/v1/systemone` wire types, validation and answer checks.
//! - [`transport`]: the transport trait and [`transport::SystemOneClient`].
//! - [`error`]: typed, fail-open errors.
//! - [`questions`]: the typed questions and answer enums.
//! - [`state`]: the bounded `state` string.
//! - [`text`]: Spanish-aware normalization.
//! - [`policy`]: the trust policy (fail-closed; `PDA-SP-0008.R2`).
//! - [`project`]: fuzzy project pre-filter and the standalone project question.
//! - [`command`]: commands, capabilities and the executor trait.
//! - [`log`]: `decisions.jsonl` with 10 MB rotation.
//! - [`eval`]: replay a log against another model and compare.
//! - [`cascade`]: intent → agent/action → engine/project, asking only what is needed.

pub mod cascade;
pub mod command;
pub mod error;
pub mod eval;
pub mod log;
pub mod policy;
pub mod project;
pub mod protocol;
pub mod questions;
pub mod state;
pub mod text;
pub mod transport;

pub use cascade::{Decision, run as run_cascade};
pub use error::DecisionError;
pub use policy::{Outcome, PolicyContext, Thresholds, policy};
pub use transport::{ClientOptions, DecisionTransport, SystemOneClient};
