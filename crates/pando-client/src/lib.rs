//! Pando HTTP + SSE API client.
//!
//! - [`config`]: [`PandoConfig`] (base URL, credentials, timeouts, local CA) and redacted secrets.
//! - [`error`]: the shared [`Error`] type.
//! - [`client`]: [`PandoClient`], the authenticated transport, and [`SseStream`].
//! - [`discovery`]: instance registry, URL policy and `/health` probe.
//! - [`sse`]: the incremental [`SseParser`].
//!
//! The core is derived from Bitácora's `pando-rs` (MIT).

pub mod client;
pub mod config;
pub mod discovery;
pub mod error;
pub mod sse;

pub use client::{PandoClient, ServerInfo, SseStream};
pub use config::{BasicAuth, PandoConfig, Token};
pub use discovery::{DiscoveredServer, Discovery};
pub use error::{Error, Result};
pub use sse::{SseEvent, SseParser};
