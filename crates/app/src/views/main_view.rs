//! Root view of the main window: frameless chrome (window border + 52 px title bar) over a
//! placeholder body until the organism lands.

use rust_i18n::t;

use crate::cells::mapping::CustomEngines;
use crate::cells::pane::OrganismPane;
use crate::chat::ChatPanel;
use crate::claude::{ClaudeBridge, extras::ExternalExtras};
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
use crate::views::layout::{self, Layout};
use crate::views::title_bar::AppTitleBar;
use crate::voice::assistant::{Assistant, AssistantEvent};
use crate::voice::controller::VoiceController;
use pando_client::ConnectionState;

/// Width of the split divider in px.
const DIVIDER: f32 = 6.0;

mod gpui_any {
    pub use crate::ui::AnyElement as E;
}

/// The window contents.
#[allow(missing_debug_implementations)]
pub struct MainView {
    state: ConnectionState,
    hub: ConnectionHub,
    url_input: Entity<InputState>,
    url_error: Option<String>,
    pane: Entity<OrganismPane>,
    voice: Entity<VoiceController>,
    assistant: Entity<Assistant>,
    _bridge: Entity<ClaudeBridge>,
    _assistant_sub: crate::ui::Subscription,
    _voice_sub: crate::ui::Subscription,
    _pane_sub: crate::ui::Subscription,
    chat: Entity<ChatPanel>,
    layout: Layout,
    /// Share of the width for the organism side in the split layout (permille).
    split: u16,
    dragging: bool,
    /// The chat's own (primary) session, remembered while the composer is routed to a cell.
    primary_session: Option<String>,
    /// Session the chat was switched to because the target is a cell.
    routed_to: Option<String>,
    _chat_sub: crate::ui::Subscription,
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
        let pane_sub = cx.observe(&pane, |this, _, cx| {
            this.sync_target(cx);
            cx.notify();
        });
        let chat_sub = cx.observe(&chat, |this, _, cx| {
            this.sync_primary_session(cx);
            cx.notify();
        });
        let saved = crate::settings_view::current(cx)
            .map(|s| s.ui)
            .unwrap_or_default();
        let (voice_config, voice_dirs) = crate::voice::startup_config();
        let voice_data_dir = voice_dirs.data_dir.clone();
        let organism = pane.read(cx).organism().clone();
        let voice = cx.new(|cx| VoiceController::new(cx, voice_config, voice_dirs, Some(organism)));
        let voice_sub = cx.observe(&voice, |_, _, cx| cx.notify());
        crate::voice::sections::register(cx, &voice);
        crate::voice::controller::register_handle(cx, &voice);
        let data_dir = voice_data_dir.clone();
        let (voice_for_assistant, pane_for_assistant) = (voice.clone(), pane.clone());
        let assistant =
            cx.new(|cx| Assistant::new(cx, voice_for_assistant, pane_for_assistant, data_dir));
        crate::voice::assistant::register_handle(cx, &assistant);
        let bridge_data = voice_data_dir.clone();
        let (bridge_pane, bridge_assistant) = (pane.clone(), assistant.downgrade());
        let bridge = cx.new(|cx| ClaudeBridge::new(cx, bridge_pane, bridge_assistant, bridge_data));
        let extras = cx.new(|cx| ExternalExtras::new(window, cx, bridge.downgrade()));
        pane.update(cx, |p, _| p.set_external_extras(extras));
        assistant.update(cx, |a, _| a.set_claude(bridge.downgrade()));
        crate::voice::host::keep_alive(cx, &voice, &assistant, &bridge, &pane);
        let assistant_sub = cx.subscribe_in(
            &assistant,
            window,
            |this: &mut Self, _, ev: &AssistantEvent, window, cx| {
                match ev {
                    AssistantEvent::Dictate(text) => {
                        this.chat
                            .update(cx, |c, cx| c.insert_dictation(text, window, cx));
                    }
                    AssistantEvent::ChatSession(id) => {
                        this.chat.update(cx, |c, cx| {
                            if c.active_session() != Some(id.as_str()) {
                                c.select_session(id, cx);
                            }
                        });
                    }
                    AssistantEvent::Project(_) => {}
                }
                cx.notify();
            },
        );
        let voice_focus = voice.read(cx).focus_handle_clone();
        window.focus(&voice_focus, cx);
        Self {
            state: ConnectionState::Connecting,
            hub,
            url_input,
            url_error,
            pane,
            voice,
            assistant,
            _bridge: bridge,
            _assistant_sub: assistant_sub,
            _voice_sub: voice_sub,
            _pane_sub: pane_sub,
            chat,
            layout: saved.layout,
            split: layout::clamp_permille(i64::from(saved.split_permille)),
            dragging: false,
            primary_session: None,
            routed_to: None,
            _chat_sub: chat_sub,
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

    /// Chat <-> organism wiring: while the target is the primary session, the chat's active
    /// session is the primary one the organism watches for pending permissions.
    fn sync_primary_session(&mut self, cx: &mut Context<Self>) {
        if self.routed_to.is_some() {
            return;
        }
        let active = self.chat.read(cx).active_session().map(str::to_owned);
        if active != self.primary_session {
            self.primary_session.clone_from(&active);
            self.pane.update(cx, |p, _| p.set_primary_session(active));
        }
    }

    /// Chat <-> organism wiring: "Hablarle" routes the composer to the cell's session by
    /// showing it in the chat; going back to the primary restores the previous session.
    fn sync_target(&mut self, cx: &mut Context<Self>) {
        let (is_cell, session) = {
            let p = self.pane.read(cx);
            (
                p.target().cell().is_some(),
                p.target().session().map(str::to_owned),
            )
        };
        match (is_cell, session) {
            (true, Some(sid)) if self.routed_to.as_deref() != Some(sid.as_str()) => {
                if self.routed_to.is_none() {
                    self.primary_session = self.chat.read(cx).active_session().map(str::to_owned);
                }
                self.routed_to = Some(sid.clone());
                self.chat.update(cx, |c, cx| c.select_session(&sid, cx));
            }
            (false, _) if self.routed_to.take().is_some() => {
                if let Some(p) = self.primary_session.clone() {
                    self.chat.update(cx, |c, cx| c.select_session(&p, cx));
                }
            }
            _ => {}
        }
    }

    /// Switches the layout and persists it.
    fn set_layout(&mut self, layout: Layout, cx: &mut Context<Self>) {
        if self.layout != layout {
            self.layout = layout;
            crate::shell::runtime::save_layout(cx, self.layout, self.split);
            cx.notify();
        }
    }

    /// Three chips: Orbe | Chat (with the pending count) | Dividida.
    fn layout_switch(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pending = self.chat.read(cx).pending_total();
        h_flex().gap(px(4.0)).children(Layout::ALL.iter().map(|l| {
            let l = *l;
            let active = self.layout == l;
            let alert = l == Layout::Chat && pending > 0 && !self.layout.shows_chat();
            let label = if alert {
                t!("chat.nav.chat_pending", count = pending).to_string()
            } else {
                crate::organism_view::tr(&format!("layout.{}", l.id()))
            };
            let weak = cx.weak_entity();
            let tone: Hsla = if alert {
                rgb(palette::ALAMO).into()
            } else if active {
                rgb(palette::MARFIL).into()
            } else {
                marfil_alpha(palette::MARFIL_ALPHA_62)
            };
            h_flex()
                .id(("layout", l as usize))
                .cursor_pointer()
                .px(px(10.0))
                .py(px(3.0))
                .rounded_full()
                .border_1()
                .border_color(if active {
                    tone
                } else {
                    marfil_alpha(palette::MARFIL_ALPHA_22)
                })
                .text_color(tone)
                .text_size(px(11.0))
                .on_click(move |_, _, cx| {
                    weak.update(cx, |this, cx| this.set_layout(l, cx)).ok();
                })
                .child(label)
        }))
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .left(self.layout_switch(cx))
            .left(
                div().children(waiting.then(|| chip(format!("? {}", t!("cells.header.waiting"))))),
            )
            .right(
                h_flex()
                    .gap_2()
                    .pr_3()
                    .child(connection_chip(chip_spec(&self.state)))
                    .children(self.assistant.read(cx).project_pill().map(chip))
                    .child(chip(self.assistant.read(cx).decision_chip()))
                    .child(chip(self.voice.read(cx).status_label())),
            );
        let body = if needs_empty_state(&self.state) {
            self.empty_state(cx).into_any_element()
        } else {
            self.body(window, cx)
        };
        window_border().child(
            v_flex()
                .id("main-view")
                .key_context(layout::KEY_CONTEXT)
                .on_action(
                    cx.listener(|this, _: &layout::ShowOrb, _, cx| {
                        this.set_layout(Layout::Orb, cx)
                    }),
                )
                .on_action(cx.listener(|this, _: &layout::ShowChat, _, cx| {
                    this.set_layout(Layout::Chat, cx)
                }))
                .on_action(cx.listener(|this, _: &layout::ShowSplit, _, cx| {
                    this.set_layout(Layout::Split, cx)
                }))
                .size_full()
                .bg(rgb(palette::BOSQUE))
                .text_color(rgb(palette::MARFIL))
                .child(title_bar)
                .child(body)
                .child(self.voice.clone()),
        )
    }
}

impl MainView {
    /// The body for the current layout. Hidden panels are not in the tree, so a hidden organism
    /// paints (and requests) no frames.
    fn body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui_any::E {
        let weak = cx.weak_entity();
        let (organism, chat) = (self.layout.shows_organism(), self.layout.shows_chat());
        let row = h_flex().flex_1().min_h_0().size_full();
        let compact = organism && chat;
        self.pane.update(cx, |p, cx| p.set_compact(compact, cx));
        if organism && chat {
            let total = f32::from(window.bounds().size.width);
            let (left, right) = layout::split_widths(total, self.split, DIVIDER);
            let down = weak;
            row.id("split")
                .on_mouse_move(cx.listener(
                    move |this, e: &crate::ui::paint::MouseMoveEvent, window, cx| {
                        if this.dragging {
                            let total = f32::from(window.bounds().size.width);
                            this.split =
                                layout::permille_from_drag(f32::from(e.position.x), 0.0, total);
                            cx.notify();
                        }
                    },
                ))
                .on_mouse_up(
                    crate::ui::paint::MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        if this.dragging {
                            this.dragging = false;
                            crate::shell::runtime::save_layout(cx, this.layout, this.split);
                        }
                    }),
                )
                .child(div().w(px(left)).h_full().child(self.pane.clone()))
                .child(
                    div()
                        .id("split-divider")
                        .w(px(DIVIDER))
                        .h_full()
                        .cursor_col_resize()
                        .bg(marfil_alpha(palette::MARFIL_ALPHA_12))
                        .on_mouse_down(crate::ui::paint::MouseButton::Left, move |_, _, cx| {
                            down.update(cx, |this, _| this.dragging = true).ok();
                        }),
                )
                .child(div().w(px(right)).h_full().child(self.chat.clone()))
                .into_any_element()
        } else if chat {
            row.child(self.chat.clone()).into_any_element()
        } else {
            row.child(self.pane.clone()).into_any_element()
        }
    }

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
        // A real tokio runtime wakes GPUI tasks from its worker threads (also while shutting
        // down), which the deterministic test scheduler otherwise rejects.
        cx.executor().allow_parking();
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
        runtime.shutdown_timeout(std::time::Duration::from_secs(5));
    }

    #[gpui_test]
    fn the_voice_stack_outlives_the_main_window(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
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
        let mut vcx = crate::ui::testing::VisualTestContext::from_window(window.into(), cx);
        vcx.run_until_parked();
        vcx.update(|window, _| window.remove_window());
        drop(view);
        cx.run_until_parked();
        // Tray/background mode: no window, yet push-to-talk and the settings sections work.
        cx.update(|cx| {
            assert!(crate::voice::host::is_alive(cx));
            assert!(crate::voice::controller::is_reachable(cx));
        });
        runtime.shutdown_timeout(std::time::Duration::from_secs(5));
    }
}
