//! Decision layer wiring of the app: the System One transport, command execution, the
//! decision engine and the spoken confirmation / approval flows (PDA-SP-0008).

pub mod confirm;
pub mod directory;
pub mod engine;
pub mod eval_cli;
pub mod executor;
pub mod health;
pub mod speech;
pub mod spoken_approvals;
pub mod transport;
