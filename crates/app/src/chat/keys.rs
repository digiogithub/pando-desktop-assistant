//! Keyboard actions of the chat. Navigation keys (arrows, Tab) are consumed by the text input's
//! own bindings before any `key_down` listener sees them, so the chat binds its own actions in
//! the `ChatPopup` / `ChatHistory` / `ChatPanel` contexts, which only apply while a popup is
//! open, the composer is empty, or focus is anywhere in the panel.

use crate::ui::{App, actions, key_binding};

actions!(
    chat_panel,
    [
        PopupUp,
        PopupDown,
        PopupAccept,
        HistoryUp,
        HistoryDown,
        ToggleAutoApprove,
    ]
);

/// Registers the chat's key bindings. Call once after `ui::init`.
pub fn init(cx: &mut App) {
    let bindings = [
        key_binding("up", Box::new(PopupUp), Some("ChatPopup > Input")),
        key_binding("down", Box::new(PopupDown), Some("ChatPopup > Input")),
        key_binding("tab", Box::new(PopupAccept), Some("ChatPopup > Input")),
        key_binding("up", Box::new(HistoryUp), Some("ChatHistory > Input")),
        key_binding("down", Box::new(HistoryDown), Some("ChatHistory > Input")),
        key_binding("shift-tab", Box::new(ToggleAutoApprove), Some("ChatPanel")),
        key_binding(
            "shift-tab",
            Box::new(ToggleAutoApprove),
            Some("ChatPanel > Input"),
        ),
    ];
    cx.bind_keys(bindings.into_iter().filter_map(|b| match b {
        Ok(b) => Some(b),
        Err(err) => {
            tracing::error!("invalid chat key binding: {err}");
            None
        }
    }));
}
