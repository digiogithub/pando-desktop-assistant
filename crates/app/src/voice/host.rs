//! App-level owner of the voice stack (PDA-T-0171).
//!
//! The controller, assistant, Claude bridge and organism pane are created by the main
//! window, but push-to-talk, hands-free, mute and the settings sections must keep working in
//! tray/background mode when that window is gone. A global holds strong handles, so closing
//! the window does not drop them; the next main window replaces the whole stack.

use crate::cells::pane::OrganismPane;
use crate::claude::ClaudeBridge;
use crate::ui::{App, Entity, Global};

use super::assistant::Assistant;
use super::controller::VoiceController;

#[allow(missing_debug_implementations)]
/// Strong handles to the running voice stack.
pub struct VoiceHost {
    _voice: Entity<VoiceController>,
    _assistant: Entity<Assistant>,
    _bridge: Entity<ClaudeBridge>,
    _pane: Entity<OrganismPane>,
}

impl Global for VoiceHost {}

/// Keeps the stack alive after its window closes, replacing any previous one.
pub fn keep_alive(
    cx: &mut App,
    voice: &Entity<VoiceController>,
    assistant: &Entity<Assistant>,
    bridge: &Entity<ClaudeBridge>,
    pane: &Entity<OrganismPane>,
) {
    cx.set_global(VoiceHost {
        _voice: voice.clone(),
        _assistant: assistant.clone(),
        _bridge: bridge.clone(),
        _pane: pane.clone(),
    });
}

/// Whether a voice stack is alive (it survives the main window).
#[must_use]
pub fn is_alive(cx: &App) -> bool {
    cx.try_global::<VoiceHost>().is_some()
}
