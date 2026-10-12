//! Layout of the chat panel: sidebar, header, body and overlays. The parts live in the sibling
//! `render_*` modules.

use rust_i18n::t;

use super::attention::{WaitingKind, waiting_queue};
use super::keys::{HistoryDown, HistoryUp, PopupAccept, PopupDown, PopupUp, ToggleAutoApprove};
use super::panel::ChatPanel;
use super::sessions::{ListRow, display_title};
use super::style::{Colors, pulse_dot};
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::Input;
use crate::ui::list::uniform_list;
use crate::ui::switch::Switch;
use crate::ui::text_edit::FontWeight;
use crate::ui::{
    AnyElement, Context, FluentBuilder as _, IconName, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, h_flex, px, v_flex,
};

/// Width of the sidebar.
pub(super) const SIDEBAR_WIDTH: f32 = 264.0;
/// Height of one sidebar row (the list is virtualized with fixed rows).
const ROW_HEIGHT: f32 = 34.0;

impl Render for ChatPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.sidebar_decided {
            // A narrow window starts without the sidebar; the header toggle brings it back.
            self.sidebar_decided = true;
            self.show_sidebar = window.viewport_size().width >= px(820.0);
        }
        if self.focus_composer {
            self.focus_composer = false;
            self.composer.update(cx, |s, cx| s.focus(window, cx));
        }
        self.sync_question_flow();
        // A dialog takes the keyboard (so typing never lands in the composer while it is up).
        let dialog = !waiting_queue(&self.model).is_empty();
        if dialog && !self.dialog_up {
            window.focus(&self.focus, cx);
        }
        self.dialog_up = dialog;
        let c = Colors::get();
        let sidebar = self.show_sidebar;
        h_flex()
            .id("chat-panel")
            .relative()
            .track_focus(&self.focus)
            .size_full()
            .bg(c.bg)
            .text_color(c.text)
            .text_size(px(13.0))
            .key_context(self.key_context(cx).as_str())
            .on_action(cx.listener(|this, _: &PopupUp, _, cx| this.popup_move(-1, cx)))
            .on_action(cx.listener(|this, _: &PopupDown, _, cx| this.popup_move(1, cx)))
            .on_action(
                cx.listener(|this, _: &PopupAccept, window, cx| this.popup_accept(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &HistoryUp, window, cx| this.history_step(true, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &HistoryDown, window, cx| {
                    this.history_step(false, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &ToggleAutoApprove, _, cx| this.toggle_auto_approve(cx)),
            )
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                this.on_key(ev, cx);
            }))
            .when(sidebar, |d| d.child(self.sidebar(&c, cx)))
            .child(self.main_column(&c, window, cx))
            // Dialogs cover the whole panel, sidebar included.
            .children(self.permission_dialog(&c, cx))
            .children(self.questions_dialog(&c, cx))
    }
}

impl ChatPanel {
    // ------------------------------------------------------------------ keyboard

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        // Escape on a dialog denies (fail-closed). Approving is only ever done by a click.
        if key == "escape" {
            if let Some(w) = waiting_queue(&self.model).into_iter().next() {
                match w.kind {
                    WaitingKind::Permission => {
                        self.decide_permission(&w.id, pando_client::approvals::Decision::Deny, cx)
                    }
                    WaitingKind::Question => self.cancel_questions(cx),
                }
                cx.stop_propagation();
                return;
            }
            if self.diff_file.take().is_some() || self.model_menu || self.usage_open {
                self.model_menu = false;
                self.usage_open = false;
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if self.mention.is_some() {
                self.mention = None;
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if !self.palette_dismissed && self.palette_open(cx) {
                self.palette_dismissed = true;
                cx.notify();
                cx.stop_propagation();
            }
        }
    }

    // ------------------------------------------------------------------ sidebar

    fn sidebar(&mut self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c = *c;
        let weak = cx.weak_entity();
        let new_btn = {
            let weak = weak.clone();
            Button::new("chat-new")
                .primary()
                .compact()
                .icon(IconName::Plus)
                .label(t!("chat.sidebar.new").to_string())
                .on_click(move |_, _, cx| {
                    weak.update(cx, |this, cx| this.new_session(cx)).ok();
                })
        };
        let rows = self.sessions.layout(self.now);
        let count = rows.len();
        let empty = self.sessions.rows().is_empty();
        let list = uniform_list(
            "chat-sessions",
            count,
            cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                let rows = this.sessions.layout(this.now);
                range
                    .filter_map(|i| rows.get(i).cloned())
                    .map(|row| this.session_row(&row, window, cx))
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.list_scroll)
        .flex_1()
        .min_h_0();
        v_flex()
            .id("chat-sidebar")
            .w(px(SIDEBAR_WIDTH))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(c.line)
            .bg(c.deep)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .px(px(14.0))
                    .h(px(44.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(c.text2)
                            .font_weight(FontWeight::MEDIUM)
                            .child(t!("chat.sidebar.title").to_string().to_uppercase()),
                    )
                    .child(new_btn),
            )
            .child(
                div()
                    .px(px(10.0))
                    .pb(px(8.0))
                    .child(Input::new(&self.search)),
            )
            .when(empty, |d| {
                let msg = match self.sessions.error() {
                    Some(e) => t!("chat.sidebar.error", error = e).to_string(),
                    None if self.sessions.is_loading() || self.service.is_none() => {
                        t!("chat.sidebar.loading").to_string()
                    }
                    None => t!("chat.sidebar.empty").to_string(),
                };
                d.child(
                    div()
                        .px(px(14.0))
                        .py(px(10.0))
                        .text_color(c.text3)
                        .text_size(px(12.0))
                        .child(msg),
                )
            })
            .child(list)
            .into_any_element()
    }

    fn session_row(
        &mut self,
        row: &ListRow,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = Colors::get();
        match row {
            ListRow::Header(b) => div()
                .h(px(ROW_HEIGHT))
                .px(px(14.0))
                .flex()
                .items_end()
                .pb(px(4.0))
                .text_size(px(10.0))
                .text_color(c.text3)
                .child(
                    t!(format!("chat.sidebar.bucket.{}", b.key()))
                        .to_string()
                        .to_uppercase(),
                )
                .into_any_element(),
            ListRow::LoadMore => {
                let weak = cx.weak_entity();
                let loading = self.sessions.is_loading();
                div()
                    .h(px(ROW_HEIGHT))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .child(
                        Button::new("chat-load-more")
                            .ghost()
                            .compact()
                            .loading(loading)
                            .label(t!("chat.sidebar.load_more").to_string())
                            .on_click(move |_, _, cx| {
                                weak.update(cx, |this, cx| this.load_more_sessions(cx)).ok();
                            }),
                    )
                    .into_any_element()
            }
            ListRow::Session(i) => {
                let Some(s) = self.sessions.rows().get(*i).cloned() else {
                    return div().h(px(ROW_HEIGHT)).into_any_element();
                };
                let id = s.session.id.clone();
                let selected = self.model.session_id() == Some(id.as_str());
                let waiting = self.tally.count(&id)
                    + usize::from(selected && !waiting_queue(&self.model).is_empty());
                let title =
                    display_title(&s).unwrap_or_else(|| t!("chat.sidebar.untitled").to_string());
                if let Some(r) = self.rename.as_ref().filter(|r| r.id == id) {
                    return h_flex()
                        .h(px(ROW_HEIGHT))
                        .px(px(8.0))
                        .items_center()
                        .child(div().flex_1().child(Input::new(&r.input)))
                        .into_any_element();
                }
                let weak = cx.weak_entity();
                let (w1, w2, w3) = (weak.clone(), weak.clone(), weak);
                let (id1, id2, id3) = (id.clone(), id.clone(), id.clone());
                let group: SharedString = format!("chat-row-{id}").into();
                let mut row = h_flex()
                    .id(SharedString::from(format!("chat-session-{id}")))
                    .relative()
                    .group(group.clone())
                    .size_full()
                    .px(px(8.0))
                    .gap(px(8.0))
                    .items_center()
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .when(selected, |d| d.bg(c.card))
                    .hover(|d| d.bg(c.hover))
                    .on_click(move |_, _, cx| {
                        w1.update(cx, |this, cx| this.select_session(&id1, cx)).ok();
                    });
                row = row.child(if s.is_running {
                    pulse_dot(format!("run-{id}"), c.accent, 7.0)
                } else {
                    div().size(px(7.0)).into_any_element()
                });
                row = row.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_size(px(13.0))
                        .text_color(if selected { c.text } else { c.text2 })
                        .child(title),
                );
                if waiting > 0 {
                    row = row.child(
                        div()
                            .px(px(6.0))
                            .rounded_full()
                            .bg(c.accent)
                            .text_color(c.on_accent)
                            .text_size(px(10.0))
                            .child(waiting.to_string()),
                    );
                } else if s.session.message_count > 0 {
                    row = row.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(c.text3)
                            .child(s.session.message_count.to_string()),
                    );
                }
                let row = row.child(
                    h_flex()
                        .absolute()
                        .right(px(4.0))
                        .top(px(3.0))
                        .gap(px(2.0))
                        .rounded(px(6.0))
                        .bg(c.card)
                        .opacity(0.0)
                        .group_hover(group, |d| d.opacity(1.0))
                        .child(
                            Button::new(SharedString::from(format!("rename-{id2}")))
                                .ghost()
                                .compact()
                                .icon(IconName::FileText)
                                .tooltip(t!("chat.sidebar.rename").to_string())
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    w2.update(cx, |this, cx| this.start_rename(&id2, window, cx))
                                        .ok();
                                }),
                        )
                        .child(
                            Button::new(SharedString::from(format!("delete-{id3}")))
                                .ghost()
                                .compact()
                                .icon(IconName::Close)
                                .tooltip(t!("chat.sidebar.delete").to_string())
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    let w = w3.clone();
                                    let id = id3.clone();
                                    crate::ui::confirm(
                                        window,
                                        cx,
                                        crate::ui::Confirmation {
                                            title: t!("chat.sidebar.delete_title").to_string(),
                                            description: t!("chat.sidebar.delete_body").to_string(),
                                            ok_text: t!("chat.sidebar.delete").to_string(),
                                            cancel_text: t!("chat.cancel").to_string(),
                                        },
                                        move |_, cx| {
                                            w.update(cx, |this, cx| this.delete_session(&id, cx))
                                                .ok();
                                        },
                                    );
                                }),
                        ),
                );
                div()
                    .w_full()
                    .h(px(ROW_HEIGHT))
                    .px(px(6.0))
                    .child(row)
                    .into_any_element()
            }
        }
    }

    // ------------------------------------------------------------------ main column

    fn main_column(
        &mut self,
        c: &Colors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = *c;
        let toast = self.toast.clone().map(|text| {
            let weak = cx.weak_entity();
            h_flex()
                .mx(px(16.0))
                .mb(px(6.0))
                .px(px(12.0))
                .py(px(6.0))
                .gap(px(8.0))
                .items_center()
                .rounded(px(8.0))
                .border_1()
                .border_color(c.danger)
                .text_color(c.danger)
                .text_size(px(12.0))
                .child(div().flex_1().child(text))
                .child(
                    Button::new("toast-close")
                        .ghost()
                        .compact()
                        .icon(IconName::Close)
                        .on_click(move |_, _, cx| {
                            weak.update(cx, |this, cx| {
                                this.toast = None;
                                cx.notify();
                            })
                            .ok();
                        }),
                )
        });
        let body = v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(self.header(&c, cx))
            .child(self.banners(&c, cx))
            .child(self.transcript(&c, window, cx))
            .children(toast)
            .child(self.changed_files_bar(&c, cx))
            .child(self.steering_chips(&c, cx))
            .child(self.composer_area(&c, window, cx));
        v_flex()
            .id("chat-main")
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(body)
            .children(self.diff_viewer(&c, cx))
            .into_any_element()
    }

    fn header(&mut self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c = *c;
        let weak = cx.weak_entity();
        let title = {
            let t = self.model.title().trim();
            if !t.is_empty() {
                t.to_owned()
            } else if let Some(s) = self.model.session_id().and_then(|id| self.sessions.get(id)) {
                display_title(s).unwrap_or_else(|| t!("chat.sidebar.untitled").to_string())
            } else {
                t!("chat.header.new").to_string()
            }
        };
        let toggle = {
            let weak = weak.clone();
            Button::new("chat-sidebar-toggle")
                .ghost()
                .compact()
                .icon(if self.show_sidebar {
                    IconName::PanelLeftClose
                } else {
                    IconName::PanelLeftOpen
                })
                .tooltip(t!("chat.header.toggle_sidebar").to_string())
                .on_click(move |_, _, cx| {
                    weak.update(cx, |this, cx| {
                        this.show_sidebar = !this.show_sidebar;
                        cx.notify();
                    })
                    .ok();
                })
        };
        let auto = {
            let weak = weak.clone();
            let on = self.auto_approve;
            h_flex()
                .gap(px(6.0))
                .items_center()
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(if on { c.accent } else { c.text3 })
                        .child(if on {
                            t!("chat.auto.active").to_string()
                        } else {
                            t!("chat.auto.label").to_string()
                        }),
                )
                .child(
                    Switch::new("chat-auto-approve")
                        .checked(on)
                        .accessibility_label(t!("chat.auto.label").to_string())
                        .tooltip(t!("chat.auto.tooltip").to_string())
                        .on_click(move |_, _, cx| {
                            weak.update(cx, |this, cx| this.toggle_auto_approve(cx))
                                .ok();
                        }),
                )
        };
        h_flex()
            .h(px(44.0))
            .flex_shrink_0()
            .px(px(12.0))
            .gap(px(10.0))
            .items_center()
            .border_b_1()
            .border_color(c.line)
            .child(toggle)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(title),
            )
            .when(self.model.is_running(), |d| {
                d.child(
                    h_flex()
                        .gap(px(6.0))
                        .items_center()
                        .text_size(px(11.0))
                        .text_color(c.accent)
                        .child(pulse_dot("header-run", c.accent, 7.0))
                        .child(t!("chat.header.running").to_string()),
                )
            })
            .child(self.pending_indicator(&c, cx))
            .child(self.usage_chip(&c, cx))
            .child(auto)
            .into_any_element()
    }
}
