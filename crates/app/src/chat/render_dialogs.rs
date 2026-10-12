//! Human-in-the-loop dialogs (PDA-T-0079, 0080) and the global pending indicator (PDA-T-0083).
//!
//! Safety: the approve buttons are the only way to approve from this UI. There is no default
//! button, no Enter shortcut and no timer that approves; Escape and the deny button deny.

use pando_client::approvals::{Decision, RequestBody};
use rust_i18n::t;

use super::attention::{GrantScope, WaitingKind, permission_view, waiting_queue};
use super::panel::ChatPanel;
use super::render_transcript::code_box;
use super::style::Colors;
use super::tools;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::Input;
use crate::ui::text_edit::FontWeight;
use crate::ui::{
    AnyElement, Context, Disableable as _, FluentBuilder as _, Hsla, IconName,
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, h_flex, icon, px, v_flex,
};

fn backdrop(c: &Colors) -> crate::ui::Div {
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(Hsla { a: 0.6, ..c.deep })
}

impl ChatPanel {
    // ------------------------------------------------------------------ pending indicator

    pub(super) fn pending_indicator(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c = *c;
        let total = self.pending_total();
        if total == 0 {
            return div().into_any_element();
        }
        let weak = cx.weak_entity();
        h_flex()
            .id("chat-pending-indicator")
            .gap(px(6.0))
            .items_center()
            .px(px(8.0))
            .py(px(2.0))
            .rounded_full()
            .bg(c.accent)
            .text_color(c.on_accent)
            .text_size(px(11.0))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                weak.update(cx, |this, cx| this.open_first_pending(cx)).ok();
            })
            .child(icon(IconName::Bell).size(px(12.0)))
            .child(t!("chat.pending.indicator", count = total).to_string())
            .into_any_element()
    }

    // ------------------------------------------------------------------ permissions

    pub(super) fn permission_dialog(
        &self,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let c = *c;
        let queue = waiting_queue(&self.model);
        let head = queue.first()?;
        if head.kind != WaitingKind::Permission {
            return None;
        }
        let RequestBody::Permission(p) = self.model.approvals().request(&head.id)? else {
            return None;
        };
        let flagged = self.model.approvals().is_flagged(&head.id);
        let v = permission_view(p, flagged);
        let extra = queue.len() - 1;
        let id = v.id.clone();
        let accent = if v.escalation || v.explicit_only {
            c.danger
        } else {
            c.accent
        };

        let weak = cx.weak_entity();
        let deny = {
            let (weak, id) = (weak.clone(), id.clone());
            Button::new("perm-deny")
                .danger()
                .label(t!("chat.permission.deny").to_string())
                .on_click(move |_, _, cx| {
                    let id = id.clone();
                    weak.update(cx, |this, cx| {
                        this.decide_permission(&id, Decision::Deny, cx)
                    })
                    .ok();
                })
        };
        let once = {
            let (weak, id) = (weak.clone(), id.clone());
            Button::new("perm-allow")
                .primary()
                .label(t!("chat.permission.allow").to_string())
                .on_click(move |_, _, cx| {
                    let id = id.clone();
                    weak.update(cx, |this, cx| {
                        this.decide_permission(&id, Decision::Approve, cx)
                    })
                    .ok();
                })
        };
        let session = v.can_session.then(|| {
            let (weak, id) = (weak.clone(), id.clone());
            Button::new("perm-allow-session")
                .label(t!("chat.permission.allow_session").to_string())
                .on_click(move |_, _, cx| {
                    let id = id.clone();
                    weak.update(cx, |this, cx| {
                        this.decide_permission(&id, Decision::ApproveSession, cx);
                    })
                    .ok();
                })
        });
        let field = |label: String, value: String, mono: bool| {
            v_flex()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(c.text3)
                        .child(label.to_uppercase()),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .when(mono, |d| d.font_family(crate::fonts::FONT_MONO))
                        .child(value),
                )
        };

        let card = v_flex()
            .id("chat-permission-dialog")
            .w(px(540.0))
            .max_w_full()
            .gap(px(12.0))
            .p(px(18.0))
            .rounded(px(14.0))
            .border_1()
            .border_color(accent)
            .bg(c.bg)
            .shadow_lg()
            .child(
                h_flex()
                    .gap(px(10.0))
                    .items_center()
                    .child(
                        icon(IconName::TriangleAlert)
                            .size(px(18.0))
                            .text_color(accent),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(if v.escalation {
                                t!("chat.permission.title_escalation").to_string()
                            } else {
                                t!("chat.permission.title").to_string()
                            }),
                    )
                    .when(extra > 0, |d| {
                        d.child(
                            div()
                                .px(px(8.0))
                                .py(px(2.0))
                                .rounded_full()
                                .border_1()
                                .border_color(c.line2)
                                .text_size(px(11.0))
                                .text_color(c.text2)
                                .child(t!("chat.permission.queued", count = extra).to_string()),
                        )
                    }),
            )
            .when(v.escalation, |d| {
                d.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(c.danger)
                        .child(t!("chat.permission.escalation_warning").to_string()),
                )
            })
            .child(field(
                t!("chat.permission.tool").to_string(),
                v.tool.clone(),
                true,
            ))
            .when(!v.description.is_empty(), |d| {
                d.child(field(
                    t!("chat.permission.description").to_string(),
                    v.description.clone(),
                    false,
                ))
            })
            .when(!v.path.is_empty(), |d| {
                let shown = tools::display_path(
                    &v.path,
                    self.project_root.as_deref(),
                    tools::home_dir().as_deref(),
                );
                // The full path stays visible under the short form: this is a safety dialog.
                d.child(field(
                    t!("chat.permission.path").to_string(),
                    shown.clone(),
                    true,
                ))
                .when(shown != v.path, |d| {
                    d.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(c.text3)
                            .font_family(crate::fonts::FONT_MONO)
                            .child(v.path.clone()),
                    )
                })
            })
            .children(v.command.clone().map(|cmd| {
                v_flex()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(c.text3)
                            .child(t!("chat.permission.command").to_string().to_uppercase()),
                    )
                    .child(code_box(&cmd, &c))
            }))
            .children(
                v.justification
                    .clone()
                    .map(|j| field(t!("chat.permission.justification").to_string(), j, false)),
            )
            .when(v.command.is_none() && !v.params.is_empty(), |d| {
                d.child(
                    v_flex()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(c.text3)
                                .child(t!("chat.permission.params").to_string().to_uppercase()),
                        )
                        .child(code_box(&truncate(&v.params, 900), &c)),
                )
            })
            .when(!matches!(v.scope, GrantScope::None) && v.can_session, |d| {
                let scope = match &v.scope {
                    GrantScope::Prefix(p) => {
                        t!("chat.permission.scope_prefix", value = p).to_string()
                    }
                    GrantScope::Exact(p) => {
                        t!("chat.permission.scope_exact", value = p).to_string()
                    }
                    GrantScope::Other(p) => {
                        t!("chat.permission.scope_other", value = p).to_string()
                    }
                    GrantScope::None => String::new(),
                };
                d.child(div().text_size(px(12.0)).text_color(c.text2).child(scope))
            })
            .when(v.explicit_only, |d| {
                d.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(c.text2)
                        .child(t!("chat.permission.explicit_only").to_string()),
                )
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(deny)
                    .children(session)
                    .child(once),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(c.text3)
                    .child(t!("chat.permission.escape_hint").to_string()),
            );
        Some(backdrop(&c).child(card).into_any_element())
    }

    // ------------------------------------------------------------------ questions

    pub(super) fn questions_dialog(
        &mut self,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let c = *c;
        let flow = self.question_flow.as_ref()?;
        let total = flow.questions().len();
        let step = flow.step();
        let weak = cx.weak_entity();

        let body: AnyElement = if flow.is_review() {
            v_flex()
                .gap(px(10.0))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(c.text3)
                        .child(t!("chat.questions.review").to_string().to_uppercase()),
                )
                .children(flow.questions().iter().enumerate().map(|(i, q)| {
                    let d = &flow.drafts()[i];
                    let mut answer: Vec<String> = d
                        .selected
                        .iter()
                        .filter_map(|o| q.options.get(*o))
                        .map(|o| o.label.clone())
                        .collect();
                    if d.other_on && !d.other_text.trim().is_empty() {
                        answer.push(d.other_text.trim().to_owned());
                    }
                    v_flex()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_color(c.text2)
                                .text_size(px(12.0))
                                .child(q.question.clone()),
                        )
                        .child(div().text_color(c.accent).child(answer.join(", ")))
                }))
                .into_any_element()
        } else if let Some(qi) = flow.current() {
            let q = flow.questions()[qi].clone();
            let draft = flow.drafts()[qi].clone();
            let multi = q.multi_select;
            v_flex()
                .gap(px(8.0))
                .when(!q.header.is_empty(), |d| {
                    d.child(
                        div()
                            .self_start()
                            .px(px(8.0))
                            .py(px(1.0))
                            .rounded_full()
                            .border_1()
                            .border_color(c.line2)
                            .text_size(px(10.0))
                            .text_color(c.text2)
                            .child(q.header.clone()),
                    )
                })
                .child(div().text_size(px(14.0)).child(q.question.clone()))
                .children(q.options.iter().enumerate().map(|(oi, o)| {
                    let selected = draft.selected.contains(&oi);
                    let weak = weak.clone();
                    option_row(
                        SharedString::from(format!("q-opt-{qi}-{oi}")),
                        &o.label,
                        &o.description,
                        selected,
                        multi,
                        &c,
                    )
                    .on_click(move |_, _, cx| {
                        weak.update(cx, |this, cx| {
                            if let Some(f) = this.question_flow.as_mut() {
                                f.toggle(qi, oi);
                            }
                            cx.notify();
                        })
                        .ok();
                    })
                }))
                .child({
                    let weak = weak.clone();
                    let on = draft.other_on;
                    option_row(
                        SharedString::from(format!("q-opt-{qi}-other")),
                        &t!("chat.questions.other"),
                        "",
                        on,
                        multi,
                        &c,
                    )
                    .on_click(move |_, window, cx| {
                        weak.update(cx, |this, cx| {
                            if let Some(f) = this.question_flow.as_mut() {
                                let now = !f.drafts()[qi].other_on;
                                f.set_other(qi, now);
                            }
                            this.other_input.update(cx, |s, cx| s.focus(window, cx));
                            cx.notify();
                        })
                        .ok();
                    })
                })
                .when(draft.other_on, |d| d.child(Input::new(&self.other_input)))
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let can_next = flow.can_next();
        let is_last = flow.is_last();
        let can_send = flow.can_send();
        let cancel = {
            let weak = weak.clone();
            Button::new("q-cancel")
                .ghost()
                .label(t!("chat.questions.cancel").to_string())
                .on_click(move |_, _, cx| {
                    weak.update(cx, |this, cx| this.cancel_questions(cx)).ok();
                })
        };
        let back = {
            let weak = weak.clone();
            Button::new("q-back")
                .label(t!("chat.questions.back").to_string())
                .disabled(step == 0)
                .on_click(move |_, window, cx| {
                    weak.update(cx, |this, cx| {
                        if let Some(f) = this.question_flow.as_mut() {
                            f.back();
                        }
                        this.sync_other_input(window, cx);
                        cx.notify();
                    })
                    .ok();
                })
        };
        let forward = {
            let weak = weak.clone();
            if is_last {
                Button::new("q-send")
                    .primary()
                    .label(t!("chat.questions.send").to_string())
                    .disabled(!can_send)
                    .on_click(move |_, _, cx| {
                        weak.update(cx, |this, cx| this.send_answers(cx)).ok();
                    })
            } else {
                Button::new("q-next")
                    .primary()
                    .label(t!("chat.questions.next").to_string())
                    .disabled(!can_next)
                    .on_click(move |_, window, cx| {
                        weak.update(cx, |this, cx| {
                            if let Some(f) = this.question_flow.as_mut() {
                                f.next();
                            }
                            this.sync_other_input(window, cx);
                            cx.notify();
                        })
                        .ok();
                    })
            }
        };
        let extra = waiting_queue(&self.model).len().saturating_sub(1);
        let card = v_flex()
            .id("chat-questions-dialog")
            .w(px(540.0))
            .max_w_full()
            .gap(px(12.0))
            .p(px(18.0))
            .rounded(px(14.0))
            .border_1()
            .border_color(c.info)
            .bg(c.bg)
            .shadow_lg()
            .child(
                h_flex()
                    .gap(px(10.0))
                    .items_center()
                    .child(icon(IconName::Info).size(px(18.0)).text_color(c.info))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(t!("chat.questions.title").to_string()),
                    )
                    .child(div().text_size(px(11.0)).text_color(c.text2).child(
                        if flow.is_review() {
                            t!("chat.questions.review").to_string()
                        } else {
                            t!("chat.questions.step", n = step + 1, total = total).to_string()
                        },
                    ))
                    .when(extra > 0, |d| {
                        d.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(c.text3)
                                .child(t!("chat.permission.queued", count = extra).to_string()),
                        )
                    }),
            )
            .child(stepper_dots(total, step, flow.has_review(), &c))
            .child(body)
            .child(
                h_flex()
                    .justify_between()
                    .child(cancel)
                    .child(h_flex().gap(px(8.0)).child(back).child(forward)),
            );
        Some(backdrop(&c).child(card).into_any_element())
    }

    /// Puts the free text of the current question's "Otro" into the input.
    pub(super) fn sync_other_input(
        &mut self,
        window: &mut crate::ui::Window,
        cx: &mut Context<Self>,
    ) {
        let text = self
            .question_flow
            .as_ref()
            .and_then(|f| f.current().map(|q| f.drafts()[q].other_text.clone()))
            .unwrap_or_default();
        self.other_input
            .update(cx, |s, cx| s.set_value(text, window, cx));
    }
}

fn option_row(
    id: SharedString,
    label: &str,
    description: &str,
    selected: bool,
    multi: bool,
    c: &Colors,
) -> crate::ui::Stateful<crate::ui::Div> {
    let c = *c;
    let mark = if multi {
        if selected { "☑" } else { "☐" }
    } else if selected {
        "◉"
    } else {
        "○"
    };
    h_flex()
        .id(id)
        .gap(px(10.0))
        .items_start()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(if selected { c.accent } else { c.line })
        .cursor_pointer()
        .hover(|d| d.bg(c.hover))
        .child(
            div()
                .text_color(if selected { c.accent } else { c.text3 })
                .child(mark),
        )
        .child(
            v_flex()
                .gap(px(2.0))
                .child(div().child(label.to_owned()))
                .when(!description.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(12.0))
                            .text_color(c.text2)
                            .child(description.to_owned()),
                    )
                }),
        )
}

fn stepper_dots(total: usize, step: usize, review: bool, c: &Colors) -> AnyElement {
    let c = *c;
    let n = if review { total + 1 } else { total };
    if n <= 1 {
        return div().into_any_element();
    }
    h_flex()
        .gap(px(6.0))
        .children((0..n).map(|i| {
            div()
                .h(px(4.0))
                .flex_1()
                .rounded_full()
                .bg(if i <= step { c.accent } else { c.line2 })
        }))
        .into_any_element()
}

fn truncate(s: &str, max: usize) -> String {
    super::tools::truncate_chars(s, max)
}
