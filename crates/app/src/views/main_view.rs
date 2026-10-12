//! Root view of the main window: frameless chrome (window border + 52 px title bar) over a
//! placeholder body until the organism lands.

use rust_i18n::t;

use crate::cells::mapping::CustomEngines;
use crate::cells::pane::OrganismPane;
use crate::chat::ChatPanel;
use crate::connection::{ChipSpec, ConnectionHub, Tone, chip_spec, needs_empty_state, reason_text};
use crate::events::EventPump;
use crate::ui::assets::WORDMARK_ON_DARK;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::frameless::window_border;
use crate::ui::input::{Input, InputState};
use crate::ui::theme::palette;
use crate::ui::{
    AppContext as _, Context, Entity, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Task, Window, div,
    h_flex, px, rgb, rgba, text_edit::img, v_flex,
};
use crate::views::title_bar::AppTitleBar;
use pando_client::ConnectionState;

/// The window contents.
#[allow(missing_debug_implementations)]
pub struct MainView {
    state: ConnectionState,
    hub: ConnectionHub,
    url_input: Entity<InputState>,
    url_error: Option<String>,
    pane: Entity<OrganismPane>,
    _pane_sub: crate::ui::Subscription,
    chat: Entity<ChatPanel>,
    /// The body shows the chat instead of the organism.
    show_chat: bool,
    _pump: Task<()>,
}

impl MainView {
    /// Builds the view and starts the connection service (`manual_url` skips discovery).
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        handle: tokio::runtime::Handle,
        manual_url: Option<String>,
    ) -> Self {
        let (tx, rx) = async_channel::unbounded();
        let chat_handle = handle.clone();
        let mut hub = ConnectionHub::new(handle.clone(), tx);
        let mut url_error = None;
        if let Err(err) = hub.start(manual_url.as_deref()) {
            tracing::warn!("ignoring the stored Pando URL: {err}");
            url_error = Some(err.0);
            // Fall back to discovery.
            let _ = hub.start(None);
        }
        let url_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("pando.empty.url_placeholder").to_string());
            if let Some(url) = &manual_url {
                state.set_value(url.clone(), window, cx);
            }
            state
        });
        let chat = cx.new(|cx| ChatPanel::new(window, cx, chat_handle));
        let pump = EventPump::attach(cx, rx, |this: &mut Self, state, cx| {
            this.state = state;
            this.sync_chat(cx);
        });
        let custom = directories::BaseDirs::new()
            .map(|d| CustomEngines::load_dir(&d.home_dir().join(".pando/mesnada/engines")))
            .unwrap_or_default();
        let snapshots = hub.subscribe();
        let reduced_motion = cx.reduce_motion();
        let pane =
            cx.new(|cx| OrganismPane::new(window, cx, &handle, snapshots, custom, reduced_motion));
        let pane_sub = cx.observe(&pane, |_, _, cx| cx.notify());
        Self {
            state: ConnectionState::Connecting,
            hub,
            url_input,
            url_error,
            pane,
            _pane_sub: pane_sub,
            chat,
            show_chat: false,
            _pump: pump,
        }
    }

    /// Gives the chat the connected client, or none while Pando is unreachable.
    fn sync_chat(&mut self, cx: &mut Context<Self>) {
        let client = if needs_empty_state(&self.state) {
            None
        } else {
            self.hub.client()
        };
        self.chat.update(cx, |chat, cx| chat.set_client(client, cx));
    }

    /// The chip that switches the body between the organism and the chat; it also carries the
    /// number of items waiting for the user, so a pending approval is visible from either view.
    fn view_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pending = self.chat.read(cx).pending_total();
        let label = if self.show_chat {
            t!("chat.nav.organism").to_string()
        } else if pending > 0 {
            t!("chat.nav.chat_pending", count = pending).to_string()
        } else {
            t!("chat.nav.chat").to_string()
        };
        let weak = cx.weak_entity();
        h_flex()
            .id("view-toggle")
            .cursor_pointer()
            .px(px(10.0))
            .py(px(3.0))
            .rounded_full()
            .border_1()
            .border_color(if pending > 0 && !self.show_chat {
                rgb(palette::ALAMO).into()
            } else {
                marfil_alpha(palette::MARFIL_ALPHA_22)
            })
            .text_color(if pending > 0 && !self.show_chat {
                rgb(palette::ALAMO).into()
            } else {
                marfil_alpha(palette::MARFIL_ALPHA_62)
            })
            .text_size(px(11.0))
            .on_click(move |_, _, cx| {
                weak.update(cx, |this, cx| {
                    this.show_chat = !this.show_chat;
                    cx.notify();
                })
                .ok();
            })
            .child(label)
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        self.url_error = None;
        self.hub.retry();
        cx.notify();
    }

    fn apply_url(&mut self, cx: &mut Context<Self>) {
        let url = self.url_input.read(cx).value().to_string();
        match self.hub.start(Some(&url)) {
            Ok(()) => self.url_error = None,
            Err(err) => self.url_error = Some(err.0),
        }
        cx.notify();
    }
}

fn marfil_alpha(c: palette::Rgba) -> Hsla {
    rgba(c.as_u32()).into()
}

/// A rounded chip with a label (placeholder for the project pill).
fn chip(label: String) -> impl IntoElement {
    div()
        .px(px(10.0))
        .py(px(3.0))
        .rounded_full()
        .border_1()
        .border_color(marfil_alpha(palette::MARFIL_ALPHA_22))
        .text_color(marfil_alpha(palette::MARFIL_ALPHA_62))
        .text_size(px(11.0))
        .child(label)
}

/// The Pando connection chip: a status dot plus the localized label.
fn connection_chip(spec: ChipSpec) -> impl IntoElement {
    let dot: Hsla = match spec.tone {
        Tone::Ok => rgb(palette::ALAMO).into(),
        Tone::Warn => rgb(palette::ALAMO_OSCURO).into(),
        Tone::Error => rgb(palette::BERMELLON).into(),
        Tone::Neutral => marfil_alpha(palette::MARFIL_ALPHA_45),
    };
    h_flex()
        .id("connection-chip")
        .gap(px(6.0))
        .items_center()
        .px(px(10.0))
        .py(px(3.0))
        .rounded_full()
        .border_1()
        .border_color(marfil_alpha(palette::MARFIL_ALPHA_22))
        .text_color(marfil_alpha(palette::MARFIL_ALPHA_62))
        .text_size(px(11.0))
        .child(div().size(px(7.0)).rounded_full().bg(dot))
        .child(spec.label)
}

impl Render for MainView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let project = match &self.state {
            ConnectionState::Connected {
                project: Some(name),
                ..
            } if !name.is_empty() => name.clone(),
            _ => t!("titlebar.project_none").to_string(),
        };
        let (target_label, waiting) = {
            let pane = self.pane.read(cx);
            (pane.target_label(), pane.has_pending())
        };
        let title_bar = AppTitleBar::new()
            .left(
                img(WORDMARK_ON_DARK)
                    .h(px(22.0))
                    .w(px(84.0))
                    .flex_shrink_0(),
            )
            .left(chip(project))
            .left(chip(target_label))
            .left(self.view_toggle(cx))
            .left(
                div().children(waiting.then(|| chip(format!("? {}", t!("cells.header.waiting"))))),
            )
            .right(
                h_flex()
                    .gap_2()
                    .pr_3()
                    .child(connection_chip(chip_spec(&self.state)))
                    .child(chip(t!("titlebar.status_voice").to_string())),
            );
        let body = if needs_empty_state(&self.state) {
            self.empty_state(cx).into_any_element()
        } else {
            div()
                .flex_1()
                .min_h_0()
                .child(if self.show_chat {
                    self.chat.clone().into_any_element()
                } else {
                    self.pane.clone().into_any_element()
                })
                .into_any_element()
        };
        window_border().child(
            v_flex()
                .size_full()
                .bg(rgb(palette::BOSQUE))
                .text_color(rgb(palette::MARFIL))
                .child(title_bar)
                .child(body),
        )
    }
}

impl MainView {
    /// The "sin Pando" screen: reason, how to start Pando, retry, manual URL and settings.
    fn empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let retry = {
            let weak = weak.clone();
            Button::new("pando-retry")
                .primary()
                .label(t!("pando.empty.retry").to_string())
                .on_click(move |_, _, cx| {
                    weak.update(cx, |this, cx| this.retry(cx)).ok();
                })
        };
        let apply = {
            let weak = weak.clone();
            Button::new("pando-url-apply")
                .label(t!("pando.empty.url_apply").to_string())
                .on_click(move |_, _, cx| {
                    weak.update(cx, |this, cx| this.apply_url(cx)).ok();
                })
        };
        let settings = Button::new("pando-settings")
            .ghost()
            .label(t!("pando.empty.settings").to_string())
            .on_click(|_, window, cx| {
                // TODO(settings view): open it once it exists.
                crate::ui::notify(
                    window,
                    cx,
                    crate::ui::Level::Info,
                    t!("pando.empty.settings_soon").to_string(),
                );
            });
        let secondary = marfil_alpha(palette::MARFIL_ALPHA_62);
        v_flex()
            .id("pando-empty-state")
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(14.0))
            .p(px(32.0))
            .child(
                div()
                    .text_size(px(19.0))
                    .child(t!("pando.empty.title").to_string()),
            )
            .child(div().text_color(secondary).child(reason_text(&self.state)))
            .child(
                div()
                    .text_color(secondary)
                    .child(t!("pando.empty.instructions").to_string()),
            )
            .child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(8.0))
                    .bg(rgb(palette::BOSQUE_DEEP))
                    .font_family(crate::fonts::FONT_MONO)
                    .child(t!("pando.empty.command").to_string()),
            )
            .child(h_flex().gap_2().child(retry).child(settings))
            .child(
                div()
                    .mt(px(16.0))
                    .text_size(px(11.0))
                    .text_color(secondary)
                    .child(t!("pando.empty.url_label").to_string()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .w(px(420.0))
                    .child(div().flex_1().child(Input::new(&self.url_input)))
                    .child(apply),
            )
            .children(self.url_error.as_ref().map(|error| {
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(palette::BERMELLON))
                    .child(t!("pando.empty.url_invalid", error = error).to_string())
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, gpui_test};

    #[gpui_test]
    fn main_view_renders_the_empty_state_without_pando(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("runtime");
        cx.update(|cx| {
            crate::ui::init(cx);
            crate::ui::kit_theme::install(cx);
        });
        let handle = runtime.handle().clone();
        let window = cx.add_window(move |window, cx| {
            MainView::new(window, cx, handle, Some("http://127.0.0.1:9".to_owned()))
        });
        let view = window.root(cx).expect("root view");
        let mut cx = crate::ui::testing::VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        // Nothing listens on port 9: whatever the state, the app area is the empty screen.
        assert!(view.read_with(&cx, |v, _| needs_empty_state(&v.state)));
        runtime.shutdown_background();
    }
}
