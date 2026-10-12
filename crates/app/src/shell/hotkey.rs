//! Global hold-to-talk shortcut with press *and* release (PDA-T-0120): `global-hotkey` on
//! Windows, macOS and X11; Wayland sessions use the portal instead (`super::portal`).
//!
//! The backend runs outside GPUI: its handler only `try_send`s a [`Command::Ptt`] into the shell
//! inbox (T-0026: 120-190 us to the pump). The manager must be created on the main thread
//! (macOS) / next to the message loop (Windows), i.e. inside `Application::run`, and it must
//! live as long as the app.

use std::str::FromStr as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_channel::Sender;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use super::controller::Msg;
use super::ipc::{Command, PttAction};

/// Per-OS default shortcut without known conflicts.
#[must_use]
pub fn default_shortcut() -> &'static str {
    // Ctrl+Alt+Space is free on Windows, macOS and the common X11 desktops.
    "ctrl+alt+space"
}

/// Parses the `global-hotkey` syntax used in the settings (`ctrl+alt+space`).
pub fn parse(shortcut: &str) -> Result<HotKey, String> {
    HotKey::from_str(shortcut.trim()).map_err(|e| format!("{shortcut:?}: {e}"))
}

/// The registered shortcut; dropping it unregisters.
pub struct HotkeyBackend {
    manager: GlobalHotKeyManager,
    current: HotKey,
    /// Id the (single) event handler listens for; updated by `rebind`.
    id: Arc<AtomicU32>,
}

impl std::fmt::Debug for HotkeyBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HotkeyBackend")
            .field("current", &self.current)
            .finish()
    }
}

impl HotkeyBackend {
    /// Registers `shortcut`; the event handler feeds `tx`. `Err` carries a user-facing reason.
    pub fn start(shortcut: &str, tx: Sender<Msg>) -> Result<Self, String> {
        let hotkey = parse(shortcut)?;
        let manager = GlobalHotKeyManager::new().map_err(|e| format!("manager: {e}"))?;
        manager
            .register(hotkey)
            .map_err(|e| format!("register: {e}"))?;
        let id = Arc::new(AtomicU32::new(hotkey.id()));
        let watched = id.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            if e.id == watched.load(Ordering::Relaxed) {
                let action = match e.state {
                    HotKeyState::Pressed => PttAction::Start,
                    HotKeyState::Released => PttAction::Stop,
                };
                let _ = tx.try_send(Msg::Command(Command::Ptt { action }));
            }
        }));
        Ok(Self {
            manager,
            current: hotkey,
            id,
        })
    }

    /// Switches to another shortcut (settings changed).
    pub fn rebind(&mut self, shortcut: &str) -> Result<(), String> {
        let new = parse(shortcut)?;
        if new == self.current {
            return Ok(());
        }
        self.manager
            .register(new)
            .map_err(|e| format!("register: {e}"))?;
        let _ = self.manager.unregister(self.current);
        self.id.store(new.id(), Ordering::Relaxed);
        self.current = new;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_user_shortcuts_parse() {
        assert!(parse(default_shortcut()).is_ok());
        assert!(parse("Ctrl+Shift+KeyP").is_ok());
        assert!(parse("alt+f9").is_ok());
        assert!(parse("not a key").is_err());
        assert!(parse("").is_err());
    }
}
