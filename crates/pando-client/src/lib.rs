//! Pando HTTP + SSE API client.
//!
//! - [`config`]: [`PandoConfig`] (base URL, credentials, timeouts, local CA) and redacted secrets.
//! - [`error`]: the shared [`Error`] type.
//! - [`auth`]: local CA, secret store (OS keychain) and Basic-auth prompt hooks.
//! - [`chat`]: sessions, history and the chat SSE protocol.
//! - [`client`]: [`PandoClient`], the authenticated transport, and [`SseStream`].
//! - [`connection`]: [`ConnectionService`] (states, health, backoff, version gate).
//! - [`discovery`]: instance registry, URL policy and `/health` probe.
//! - [`sse`]: the incremental [`SseParser`].
//!
//! The core is derived from Bitácora's `pando-rs` (MIT).

pub mod auth;
pub mod chat;
pub mod client;
pub mod config;
pub mod connection;
pub mod discovery;
pub mod error;
pub mod sse;
pub mod stream;

#[cfg(feature = "keyring-store")]
pub use auth::KeyringStore;
pub use auth::{AuthHooks, CredentialPrompt, MemorySecretStore, SecretStore};
pub use chat::{
    ChatEvent, ChatStream, FoldedItem, FoldedMessage, Message, Part, PermissionAction, Session,
    SessionDetail, fold_messages,
};
pub use client::{PandoClient, ServerInfo, SseStream};
pub use config::{BasicAuth, PandoConfig, Token};
pub use connection::{ConnectionOptions, ConnectionService, ConnectionState, Link, Version};
pub use discovery::{DiscoveredServer, Discovery};
pub use error::{Error, Result};
pub use sse::{SseEvent, SseParser};
pub use stream::{EventStream, EventStreamConfig, StreamItem};
