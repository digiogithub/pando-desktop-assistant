//! Claude Code interop: hook receiver, hook installer, cell model and cross-session messaging.
//!
//! * [`events`]: typed Claude Code hook payloads ([`HookEvent`]).
//! * [`runtime`]: random port + shared secret, written to 0600 runtime files.
//! * [`receiver`]: loopback HTTP server (`POST /claude/hook`) that authenticates and forwards
//!   events (PDA-T-0130).
//! * [`installer`]: idempotent merge of the forwarder hooks into a Claude Code `settings.json`
//!   (PDA-T-0131).
//! * [`cells`]: pure hook-event to `ClaudeExternal` cell mapping for the app to render
//!   (PDA-T-0132).
//! * [`messaging`]: one-way plain-text send to a session's inbox socket (PDA-T-0133).
//!
//! Sources for the payload and settings formats are in
//! `docs/05-claude-code-interop.md` and the change note
//! `pando-assistant/changes/claude-bridge-hooks.md`.

pub mod cells;
pub mod events;
pub mod installer;
pub mod messaging;
pub mod receiver;
pub mod runtime;

pub use cells::{CellStatus, CellUpdate, ExternalCell, ProjectRef, SessionTracker};
pub use events::{HookEvent, HookKind, MessagingEndpoint};
pub use installer::{HookInstaller, InstallReport, InstallStatus};
pub use messaging::{Delivery, SendError, send_message};
pub use receiver::{HookServer, ReceiverError};
pub use runtime::RuntimeFiles;

/// Crate name, used by the skeleton smoke test.
pub const CRATE_NAME: &str = "claude-bridge";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_set() {
        assert_eq!(CRATE_NAME, "claude-bridge");
    }
}
