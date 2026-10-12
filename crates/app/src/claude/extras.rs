//! The extra block of the side card for an external Claude Code session: a message field with
//! a Send button and the "avísame al terminar" toggle (PDA-T-0132/0134).

use rust_i18n::t;

use super::{ClaudeBridge, session_of_cell};
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::{Input, InputState};
use crate::ui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, WeakEntity, Window, div, px, rgba, v_flex,
};

/// The view.
#[allow(missing_debug_implementations)]
pub struct ExternalExtras {
    input: Entity<InputState>,
    bridge: WeakEntity<ClaudeBridge>,
    /// The selected cell id (`claude:<session>`), set by the pane.
    pub cell: Option<String>,
}

impl ExternalExtras {
    /// Builds the view.
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        bridge: WeakEntity<ClaudeBridge>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("claude.card.message_placeholder").to_string())
        });
        Self {
            input,
            bridge,
            cell: None,
        }
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self
            .cell
            .as_deref()
            .and_then(session_of_cell)
            .map(str::to_owned)
        else {
            return;
        };
        let text = self.input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        self.input.update(cx, |i, cx| i.set_value("", window, cx));
        self.bridge
            .update(cx, |b, cx| b.send(&session, text, cx))
            .ok();
    }

    fn toggle_notify(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self
            .cell
            .as_deref()
            .and_then(session_of_cell)
            .map(str::to_owned)
        else {
            return;
        };
        self.bridge
            .update(cx, |b, cx| {
                let on = !b.notify_armed(&session);
                b.set_notify(&session, on, cx);
            })
            .ok();
        cx.notify();
    }
}

impl Render for ExternalExtras {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let armed = self
            .cell
            .as_deref()
            .and_then(session_of_cell)
            .zip(self.bridge.upgrade())
            .is_some_and(|(s, b)| b.read(cx).notify_armed(s));
        v_flex()
            .id("external-extras")
            .gap(px(6.0))
            .mt(px(8.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgba(0xF4F1E89E))
                    .child(t!("claude.card.message").to_string()),
            )
            .child(Input::new(&self.input))
            .child(
                Button::new("claude-send")
                    .primary()
                    .label(t!("claude.card.send").to_string())
                    .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
            )
            .child(
                Button::new("claude-notify")
                    .label(if armed {
                        t!("claude.card.notify_on").to_string()
                    } else {
                        t!("claude.card.notify_off").to_string()
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_notify(cx))),
            )
    }
}
