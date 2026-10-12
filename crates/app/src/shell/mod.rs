//! Desktop shell integration: single instance and IPC, push-to-talk triggers, tray, autostart,
//! mini window, shortcuts and notifications (PDA-EP-0008). Everything here talks to the GPUI
//! side through `async_channel`s, never the other way round.

pub mod autostart;
pub mod controller;
pub mod hotkey;
pub mod ipc;
pub mod keymap;
pub mod mini;
pub mod notify;
pub mod portal;
pub mod ptt;
pub mod runtime;
pub mod tray;
