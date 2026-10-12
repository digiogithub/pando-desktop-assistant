//! The native chat (epic PDA-EP-0006) and its human-in-the-loop dialogs (PDA-EP-0007).
//!
//! Pure logic lives in `markdown`, `tools`, `diff`, `sessions`, `composer` and `attention` (all
//! unit-tested without GPUI); `service` talks to Pando on tokio; `panel` holds the state and the
//! `render_*` modules draw it.

pub mod attention;
pub mod composer;
pub mod diff;
pub mod markdown;
pub mod service;
pub mod sessions;
pub mod style;
pub mod tools;

mod keys;
mod md_view;
mod panel;
mod render;
mod render_banners;
mod render_composer;
mod render_dialogs;
mod render_transcript;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod testutil;

pub use keys::init;
pub use panel::ChatPanel;
