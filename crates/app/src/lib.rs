//! Library half of the `pando-assistant` binary (kept as a lib so tests can reach every module).

// Loads `assets/locales/*.yml` (`<namespace>.<lang>.yml`); shared with GPUI Kit (same
// `rust-i18n` instance). Spanish is the source locale (ADR-0003).
rust_i18n::i18n!("assets/locales", fallback = "es");

pub mod about;
pub mod app;
pub mod cells;
pub mod chat;
pub mod cli;
pub mod connection;
pub mod events;
pub mod firstrun;
pub mod fonts;
pub mod i18n;
#[cfg(test)]
mod i18n_audit;
pub mod logging;
pub mod organism_view;
pub mod os_prefs;
pub mod paths;
pub mod settings;
pub mod settings_view;
pub mod shell;
pub mod tokio_bridge;
pub mod ui;
pub mod update;
pub mod views;
pub mod windows;
