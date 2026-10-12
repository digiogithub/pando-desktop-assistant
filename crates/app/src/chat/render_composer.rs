//! Composer: textarea, send/stop, character limit, slash palette, `@` mentions, model selector,
//! context meter and the steering chips (PDA-T-0073, 0074, 0075, 0076).

use pando_client::api::{CommandInfo, ModelInfo};
use pando_client::steering::ChipState;
use rust_i18n::t;

use super::composer::{self, Counter, MeterLevel};
use super::panel::ChatPanel;
use super::style::Colors;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::{Input, Textarea};
use crate::ui::text_edit::{FontWeight, relative};
use crate::ui::{
    AnyElement, Context, Disableable as _, FluentBuilder as _, IconName, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, h_flex, icon, px, v_flex,
};

/// Palette rows shown at most.
const PALETTE_MAX: usize = 8;

impl ChatPanel {
    /// Commands that match what is typed after the leading `/`.
    pub(super) fn palette_matches(&self, text: &str) -> Vec<&CommandInfo> {
        composer::slash_query(text)
            .map(|q| composer::filter_commands(&self.commands, q))
            .unwrap_or_default()
    }

    pub(super) fn palette_open(&self, cx: &crate::ui::App) -> bool {
        if self.palette_dismissed {
            return false;
        }
        let text = self.composer.read(cx).value().to_string();
        !self.palette_matches(&text).is_empty()
    }

    // ------------------------------------------------------------------ keyboard

    /// A popup (slash palette or mention results) is on screen and owns the arrows.
    pub(super) fn popup_open(&self, cx: &crate::ui::App) -> bool {
        self.mention.as_ref().is_some_and(|m| !m.results.is_empty()) || self.palette_open(cx)
    }

    /// The key context of the panel: which of the chat's own bindings apply right now.
    pub(super) fn key_context(&self, cx: &crate::ui::App) -> String {
        let mut ctx = String::from("ChatPanel");
        if self.popup_open(cx) {
            ctx.push_str(" ChatPopup");
        } else {
            let empty = self.composer.read(cx).value().is_empty();
            if empty || self.history.is_browsing() {
                ctx.push_str(" ChatHistory");
            }
        }
        ctx
    }

    /// Up/Down inside a popup.
    pub(super) fn popup_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(m) = self.mention.as_mut()
            && !m.results.is_empty()
        {
            m.index = m
                .index
                .saturating_add_signed(delta)
                .min(m.results.len() - 1);
        } else {
            let text = self.composer.read(cx).value().to_string();
            let n = self.palette_matches(&text).len();
            self.palette_ix = self
                .palette_ix
                .saturating_add_signed(delta)
                .min(n.saturating_sub(1));
        }
        cx.notify();
    }

    /// Tab inside a popup picks the highlighted entry.
    pub(super) fn popup_accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(m) = &self.mention
            && let Some(path) = m.results.get(m.index).cloned()
        {
            self.pick_mention(&path, window, cx);
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        let name = self
            .palette_matches(&text)
            .get(self.palette_ix)
            .map(|c| c.name.clone());
        if let Some(name) = name {
            self.pick_command(&name, window, cx);
        }
    }

    /// Up/Down on an empty composer walks the prompt history.
    pub(super) fn history_step(&mut self, up: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        let next = if up {
            self.history.older(&text).map(str::to_owned)
        } else {
            self.history.newer().map(str::to_owned)
        };
        if let Some(t) = next {
            self.set_composer_text(&t, window, cx);
        }
    }

    // ------------------------------------------------------------------ steering chips

    pub(super) fn steering_chips(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c = *c;
        let chips = self.model.steering().chips();
        if chips.is_empty() {
            return div().into_any_element();
        }
        v_flex()
            .px(px(16.0))
            .pb(px(6.0))
            .gap(px(4.0))
            .children(chips.iter().map(|chip| {
                let weak = cx.weak_entity();
                let cid = chip.id;
                let (label, color) = match &chip.state {
                    ChipState::Sending => (t!("chat.steering.sending").to_string(), c.text3),
                    ChipState::Queued => (t!("chat.steering.queued").to_string(), c.accent),
                    ChipState::Failed(e) => {
                        (t!("chat.steering.failed", error = e).to_string(), c.danger)
                    }
                };
                let failed = matches!(chip.state, ChipState::Failed(_));
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(color)
                    .text_size(px(12.0))
                    .child(div().text_color(color).child(label))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(c.text2)
                            .child(chip.text.clone()),
                    )
                    .when(failed, |d| {
                        d.child(
                            Button::new(SharedString::from(format!("chip-discard-{}", cid.0)))
                                .ghost()
                                .compact()
                                .label(t!("chat.steering.discard").to_string())
                                .on_click(move |_, _, cx| {
                                    weak.update(cx, |this, cx| {
                                        this.model.steering_mut().discard(cid);
                                        cx.notify();
                                    })
                                    .ok();
                                }),
                        )
                    })
            }))
            .into_any_element()
    }

    // ------------------------------------------------------------------ composer

    pub(super) fn composer_area(
        &mut self,
        c: &Colors,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = *c;
        let (text, _) = self.composer_text(cx);
        let running = self.model.is_running();
        let sendable = composer::can_send(&text) && self.service.is_some();
        let weak = cx.weak_entity();

        let send = {
            let weak = weak.clone();
            Button::new("chat-send")
                .primary()
                .compact()
                .icon(IconName::ArrowUp)
                .disabled(!sendable)
                .tooltip(if running {
                    t!("chat.composer.steer").to_string()
                } else {
                    t!("chat.composer.send").to_string()
                })
                .on_click(move |_, window, cx| {
                    weak.update(cx, |this, cx| this.send(window, cx)).ok();
                })
        };
        let stop = {
            let weak = weak.clone();
            Button::new("chat-stop")
                .danger()
                .compact()
                .icon(IconName::Square)
                .label(t!("chat.composer.stop").to_string())
                .on_click(move |_, _, cx| {
                    weak.update(cx, |this, cx| this.stop(cx)).ok();
                })
        };
        let counter = match composer::counter(&text) {
            Counter::Hidden => None,
            Counter::Shown(n) => Some((n, c.text3)),
            Counter::Over(n) => Some((n, c.danger)),
        };
        let dictated = self.dictated.is_some();

        let boxed = v_flex()
            .rounded(px(14.0))
            .border_1()
            .border_color(c.line2)
            .bg(c.deep)
            .px(px(12.0))
            .pt(px(8.0))
            .pb(px(6.0))
            .gap(px(6.0))
            .child(
                Textarea::new(&self.composer)
                    .appearance(false)
                    .bordered(false),
            )
            .child(
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(self.model_selector(&c, cx))
                    .child(self.context_meter(&c))
                    .child(div().flex_1())
                    .when(dictated, |d| {
                        d.child(
                            h_flex()
                                .gap(px(4.0))
                                .items_center()
                                .px(px(8.0))
                                .rounded_full()
                                .border_1()
                                .border_color(c.info)
                                .text_color(c.info)
                                .text_size(px(10.0))
                                .child(icon(IconName::Mic).size(px(11.0)))
                                .child(t!("chat.composer.dictated").to_string()),
                        )
                    })
                    .children(counter.map(|(n, color)| {
                        div()
                            .text_size(px(11.0))
                            .text_color(color)
                            .child(format!("{n}/{}", composer::LIMIT))
                    }))
                    .when(running, |d| d.child(stop))
                    .child(send),
            );
        v_flex()
            .relative()
            .flex_shrink_0()
            .px(px(16.0))
            .pb(px(12.0))
            .pt(px(4.0))
            .child(boxed)
            .children(self.palette_popup(&text, &c, cx))
            .children(self.mention_popup(&c, cx))
            .children(self.model_menu_popup(&c, cx))
            .into_any_element()
    }

    // ------------------------------------------------------------------ popups

    /// A popup floating right above the composer box.
    fn popup_shell(c: &Colors) -> crate::ui::Div {
        v_flex()
            .absolute()
            .left(px(16.0))
            .right(px(16.0))
            .bottom(relative(1.0))
            .mb(px(-2.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(c.line2)
            .bg(c.bg)
            .shadow_lg()
            .py(px(4.0))
            .overflow_hidden()
    }

    fn palette_popup(&self, text: &str, c: &Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.palette_dismissed {
            return None;
        }
        let matches = self.palette_matches(text);
        if matches.is_empty() {
            return None;
        }
        let c = *c;
        let sel = self.palette_ix.min(matches.len() - 1);
        Some(
            Self::popup_shell(&c)
                .id("chat-palette")
                .children(
                    matches
                        .iter()
                        .take(PALETTE_MAX)
                        .enumerate()
                        .map(|(i, cmd)| {
                            let weak = cx.weak_entity();
                            let name = cmd.name.clone();
                            h_flex()
                                .id(SharedString::from(format!("cmd-{}", cmd.name)))
                                .gap(px(10.0))
                                .px(px(12.0))
                                .py(px(5.0))
                                .cursor_pointer()
                                .when(i == sel, |d| d.bg(c.card))
                                .hover(|d| d.bg(c.hover))
                                .on_click(move |_, window, cx| {
                                    let name = name.clone();
                                    weak.update(cx, |this, cx| {
                                        this.pick_command(&name, window, cx)
                                    })
                                    .ok();
                                })
                                .child(
                                    div()
                                        .font_family(crate::fonts::FONT_MONO)
                                        .text_color(c.accent)
                                        .child(format!("/{}", cmd.name)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(c.text2)
                                        .text_size(px(12.0))
                                        .child(cmd.description.clone()),
                                )
                                .when(cmd.accepts_args, |d| {
                                    d.child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(c.text3)
                                            .child(t!("chat.composer.accepts_args").to_string()),
                                    )
                                })
                        }),
                )
                .into_any_element(),
        )
    }

    fn mention_popup(&self, c: &Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let m = self.mention.as_ref()?;
        let c = *c;
        if m.results.is_empty() {
            if m.mention.query.is_empty() {
                return Some(
                    Self::popup_shell(&c)
                        .child(
                            div()
                                .px(px(12.0))
                                .py(px(6.0))
                                .text_size(px(12.0))
                                .text_color(c.text3)
                                .child(t!("chat.mention.hint").to_string()),
                        )
                        .into_any_element(),
                );
            }
            return None;
        }
        Some(
            Self::popup_shell(&c)
                .id("chat-mention")
                .children(
                    m.results
                        .iter()
                        .take(PALETTE_MAX)
                        .enumerate()
                        .map(|(i, path)| {
                            let weak = cx.weak_entity();
                            let p = path.clone();
                            h_flex()
                                .id(SharedString::from(format!("mention-{path}")))
                                .gap(px(8.0))
                                .px(px(12.0))
                                .py(px(5.0))
                                .cursor_pointer()
                                .when(i == m.index, |d| d.bg(c.card))
                                .hover(|d| d.bg(c.hover))
                                .on_click(move |_, window, cx| {
                                    let p = p.clone();
                                    weak.update(cx, |this, cx| this.pick_mention(&p, window, cx))
                                        .ok();
                                })
                                .child(icon(IconName::File).size(px(13.0)).text_color(c.text3))
                                .child(
                                    div()
                                        .font_family(crate::fonts::FONT_MONO)
                                        .text_size(px(12.0))
                                        .child(path.clone()),
                                )
                        }),
                )
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------------ model selector

    fn current_model_label(&self) -> String {
        let id = self
            .session_model
            .clone()
            .or_else(|| self.pending_model.clone());
        match id {
            Some(id) => self
                .models
                .as_ref()
                .and_then(|l| l.models.iter().find(|m| m.id == id))
                .map_or(id, display_model),
            None => t!("chat.model.default").to_string(),
        }
    }

    fn model_selector(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c = *c;
        let weak = cx.weak_entity();
        let open = self.model_menu;
        h_flex()
            .id("chat-model-selector")
            .gap(px(5.0))
            .items_center()
            .px(px(8.0))
            .py(px(3.0))
            .rounded_full()
            .border_1()
            .border_color(if open { c.accent } else { c.line2 })
            .cursor_pointer()
            .hover(|d| d.bg(c.hover))
            .text_size(px(11.0))
            .text_color(c.text2)
            .on_click(move |_, _, cx| {
                weak.update(cx, |this, cx| {
                    this.model_menu = !this.model_menu;
                    cx.notify();
                })
                .ok();
            })
            .child(icon(IconName::Cpu).size(px(12.0)))
            .child(self.current_model_label())
            .child(icon(IconName::ChevronDown).size(px(11.0)))
            .into_any_element()
    }

    fn model_menu_popup(&self, c: &Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.model_menu {
            return None;
        }
        let c = *c;
        let filter = self.model_filter.read(cx).value().to_lowercase();
        let current = self
            .session_model
            .clone()
            .or_else(|| self.pending_model.clone());
        let mut body = v_flex().max_h(px(280.0)).overflow_hidden();
        match &self.models {
            None => {
                body = body.child(
                    div()
                        .px(px(12.0))
                        .py(px(8.0))
                        .text_color(c.text3)
                        .child(t!("chat.model.loading").to_string()),
                );
            }
            Some(list) => {
                // `auto` first, then providers in the order Pando lists them.
                let mut groups: Vec<(String, Vec<&ModelInfo>)> = Vec::new();
                for m in list.models.iter().filter(|m| {
                    filter.is_empty()
                        || m.id.to_lowercase().contains(&filter)
                        || m.name.to_lowercase().contains(&filter)
                        || m.provider.to_lowercase().contains(&filter)
                }) {
                    let g = if m.id == "auto" {
                        String::new()
                    } else {
                        m.provider.clone()
                    };
                    match groups.iter_mut().find(|(p, _)| *p == g) {
                        Some((_, v)) => v.push(m),
                        None => groups.push((g, vec![m])),
                    }
                }
                groups.sort_by_key(|(p, _)| !p.is_empty());
                for (provider, models) in groups {
                    if !provider.is_empty() {
                        body = body.child(
                            div()
                                .px(px(12.0))
                                .pt(px(6.0))
                                .text_size(px(10.0))
                                .text_color(c.text3)
                                .child(provider.to_uppercase()),
                        );
                    }
                    for m in models {
                        let weak = cx.weak_entity();
                        let id = m.id.clone();
                        let selected = current.as_deref() == Some(m.id.as_str());
                        body = body.child(
                            h_flex()
                                .id(SharedString::from(format!("model-{}", m.id)))
                                .gap(px(8.0))
                                .px(px(12.0))
                                .py(px(5.0))
                                .cursor_pointer()
                                .when(selected, |d| d.bg(c.card))
                                .hover(|d| d.bg(c.hover))
                                .on_click(move |_, _, cx| {
                                    let id = id.clone();
                                    weak.update(cx, |this, cx| this.select_model(&id, cx)).ok();
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .font_weight(if selected {
                                            FontWeight::MEDIUM
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .child(display_model(m)),
                                )
                                .children(m.badges.iter().take(3).map(|b| {
                                    div()
                                        .px(px(6.0))
                                        .rounded_full()
                                        .border_1()
                                        .border_color(c.line2)
                                        .text_size(px(9.0))
                                        .text_color(c.text3)
                                        .child(b.clone())
                                }))
                                .when(selected, |d| {
                                    d.child(
                                        icon(IconName::Check).size(px(13.0)).text_color(c.accent),
                                    )
                                }),
                        );
                    }
                }
            }
        }
        Some(
            Self::popup_shell(&c)
                .id("chat-model-menu")
                .right(px(0.0))
                .w(px(380.0))
                .left(px(16.0))
                .child(
                    div()
                        .px(px(8.0))
                        .pb(px(4.0))
                        .child(Input::new(&self.model_filter)),
                )
                .child(body)
                .child(
                    div()
                        .px(px(12.0))
                        .pt(px(4.0))
                        .text_size(px(10.0))
                        .text_color(c.text3)
                        .child(t!("chat.model.session_only").to_string()),
                )
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------------ context meter

    fn context_meter(&self, c: &Colors) -> AnyElement {
        let c = *c;
        let Some(u) = self.model.usage().current() else {
            return div().into_any_element();
        };
        let used = u.prompt_tokens + u.cache_read_tokens + u.cache_creation_tokens;
        let Some((ratio, level)) = composer::context_meter(used, u.context_window) else {
            return div().into_any_element();
        };
        let color = match level {
            MeterLevel::Ok => c.text3,
            MeterLevel::Warning => c.accent,
            MeterLevel::Danger => c.danger,
        };
        h_flex()
            .gap(px(6.0))
            .items_center()
            .text_size(px(11.0))
            .text_color(color)
            .child(
                div()
                    .w(px(54.0))
                    .h(px(4.0))
                    .rounded_full()
                    .bg(c.line2)
                    .child(div().h_full().w(px(54.0 * ratio)).rounded_full().bg(color)),
            )
            .child(format!(
                "{} / {}",
                composer::compact_tokens(used),
                composer::compact_tokens(u.context_window)
            ))
            .into_any_element()
    }
}

fn display_model(m: &ModelInfo) -> String {
    if m.name.is_empty() {
        m.id.clone()
    } else {
        m.name.clone()
    }
}
