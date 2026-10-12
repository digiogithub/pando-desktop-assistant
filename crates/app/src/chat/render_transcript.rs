//! The transcript: bubbles, streaming markdown, thinking, notices, tool cards and activity groups
//! (PDA-T-0066, 0067, 0069, 0070).

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use pando_client::model::{Item, ItemId, ItemKind, NoticeKind, ToolItem, ToolStatus};
use rust_i18n::t;

use super::diff::{LineKind, tool_diff};
use super::markdown;
use super::md_view::{self, BODY};
use super::panel::{ChatPanel, MdEntry};
use super::style::{Colors, pulse_dot};
use super::tools::{self, Row, ToolClass};
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::shimmer::ShimmerText;
use crate::ui::text_edit::{
    ClipboardItem, FontWeight, Image, ImageFormat, ObjectFit, StyledImage as _, img,
};
use crate::ui::{
    AnyElement, Context, FluentBuilder as _, IconName, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    h_flex, icon, px, v_flex,
};

/// Widest the conversation column grows.
const COLUMN_MAX: f32 = 820.0;
/// Lines of a diff a card shows before offering the viewer.
const DIFF_PREVIEW_LINES: usize = 60;

fn blocks_for(
    md: &mut HashMap<ItemId, MdEntry>,
    id: ItemId,
    text: &str,
) -> Rc<Vec<markdown::Block>> {
    if let Some((len, blocks)) = md.get(&id)
        && *len == text.len()
    {
        return blocks.clone();
    }
    let blocks = Rc::new(markdown::parse(text));
    md.insert(id, (text.len(), blocks.clone()));
    blocks
}

impl ChatPanel {
    pub(super) fn transcript(
        &mut self,
        c: &Colors,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = *c;
        // Follow the tail while it is on; the offset catches up once the layout is known.
        if self.tail {
            self.scroll.scroll_to_bottom();
            let max = self.scroll.max_offset().y;
            if -self.scroll.offset().y + px(2.0) < max {
                cx.notify();
            }
        }
        let weak = cx.weak_entity();
        if self.model.items().is_empty() {
            return self.empty_state(&c);
        }
        let rows = tools::layout(self.model.items());
        let mut md = std::mem::take(&mut self.md);
        let mut col = v_flex()
            .w_full()
            .max_w(px(COLUMN_MAX))
            .mx_auto()
            .gap(px(14.0))
            .px(px(20.0))
            .py(px(16.0));
        for row in &rows {
            col = col.child(self.row_el(row, &mut md, &c, cx));
        }
        self.md = md;
        let scroll = div()
            .id("chat-scroll")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(cx.listener(
                |this, ev: &crate::ui::canvas::ScrollWheelEvent, _, cx| {
                    let dy = ev.delta.pixel_delta(px(16.0)).y;
                    if dy > px(0.0) {
                        // Scrolling up leaves the stream.
                        if this.tail {
                            this.tail = false;
                            cx.notify();
                        }
                    } else if dy < px(0.0) {
                        let max = this.scroll.max_offset().y;
                        if -this.scroll.offset().y + px(48.0) >= max && !this.tail {
                            this.tail = true;
                            cx.notify();
                        }
                    }
                },
            ))
            .child(col);
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(scroll)
            .when(!self.tail, |d| {
                d.child(
                    div()
                        .absolute()
                        .bottom(px(10.0))
                        .w_full()
                        .flex()
                        .justify_center()
                        .child(
                            Button::new("chat-to-end")
                                .primary()
                                .compact()
                                .icon(IconName::ArrowDown)
                                .label(t!("chat.transcript.to_end").to_string())
                                .on_click(move |_, _, cx| {
                                    weak.update(cx, |this, cx| {
                                        this.tail = true;
                                        cx.notify();
                                    })
                                    .ok();
                                }),
                        ),
                )
            })
            .into_any_element()
    }

    fn empty_state(&self, c: &Colors) -> AnyElement {
        let msg = if self.loading {
            t!("chat.transcript.loading").to_string()
        } else if self.service.is_none() {
            t!("chat.transcript.offline").to_string()
        } else {
            t!("chat.transcript.empty").to_string()
        };
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(19.0))
                    .font_family(crate::fonts::FONT_DISPLAY)
                    .child(t!("chat.transcript.empty_title").to_string()),
            )
            .child(div().text_color(c.text2).child(msg))
            .into_any_element()
    }

    fn row_el(
        &self,
        row: &Row,
        md: &mut HashMap<ItemId, MdEntry>,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match row {
            Row::Item(i) => match self.model.items().get(*i) {
                Some(item) => self.item_el(item, md, c, cx),
                None => div().into_any_element(),
            },
            Row::Activity(ix) => self.activity_el(ix, c, cx),
        }
    }

    fn item_el(
        &self,
        item: &Item,
        md: &mut HashMap<ItemId, MdEntry>,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = *c;
        let id = item.id;
        match &item.kind {
            ItemKind::User { text, steered } => h_flex()
                .justify_end()
                .child(
                    v_flex()
                        .max_w(px(580.0))
                        .gap(px(4.0))
                        .items_end()
                        .when(*steered, |d| {
                            d.child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(c.accent)
                                    .child(t!("chat.transcript.steered").to_string()),
                            )
                        })
                        .child(
                            div()
                                .px(px(14.0))
                                .py(px(9.0))
                                .rounded(px(14.0))
                                .bg(c.card)
                                .border_1()
                                .border_color(c.line2)
                                .text_size(px(BODY))
                                .line_height(px(BODY * 1.5))
                                .child(text.clone()),
                        ),
                )
                .into_any_element(),
            ItemKind::Assistant { text, streaming } => {
                let blocks = blocks_for(md, id, text);
                let copy = text.clone();
                let group: SharedString = format!("msg-{}", id.0).into();
                v_flex()
                    .relative()
                    .group(group.clone())
                    .gap(px(4.0))
                    .child(md_view::blocks_el(&blocks, id.0, &c, BODY))
                    .child(if *streaming {
                        h_flex()
                            .h(px(16.0))
                            .items_center()
                            .child(pulse_dot(format!("stream-{}", id.0), c.accent, 6.0))
                            .into_any_element()
                    } else {
                        div()
                            .absolute()
                            .top(px(-4.0))
                            .right(px(0.0))
                            .opacity(0.0)
                            .group_hover(group, |d| d.opacity(1.0))
                            .child(
                                Button::new(SharedString::from(format!("copy-{}", id.0)))
                                    .ghost()
                                    .compact()
                                    .icon(IconName::Copy)
                                    .tooltip(t!("chat.transcript.copy").to_string())
                                    .on_click(move |_, _, app| {
                                        app.write_to_clipboard(ClipboardItem::new_string(
                                            copy.clone(),
                                        ));
                                    }),
                            )
                            .into_any_element()
                    })
                    .into_any_element()
            }
            ItemKind::Thinking { text, streaming } => {
                self.thinking_el(id, text, *streaming, md, &c, cx)
            }
            ItemKind::Tool(_) => div().into_any_element(), // folded into activity rows
            ItemKind::Approval { request_id } => self.approval_row(request_id, &c),
            ItemKind::Notice { kind, text } => notice_el(kind, text, &c),
            ItemKind::Conclusion(r) => {
                let status_color = match r.status.as_str() {
                    "completed" | "success" | "done" => c.add_fg,
                    "failed" | "error" => c.danger,
                    _ => c.accent,
                };
                v_flex()
                    .gap(px(6.0))
                    .p(px(12.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(c.line2)
                    .bg(c.card)
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .items_center()
                            .text_size(px(11.0))
                            .child(icon(IconName::Bot).size(px(14.0)).text_color(c.info))
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(t!("chat.conclusion.title").to_string()),
                            )
                            .child(div().text_color(c.text3).child(format!(
                                "{} · {}",
                                r.engine,
                                if r.project.is_empty() {
                                    &r.task
                                } else {
                                    &r.project
                                }
                            )))
                            .child(div().text_color(status_color).child(r.status.clone())),
                    )
                    .child(div().text_size(px(13.0)).child(if r.no_conclusion {
                        t!("chat.conclusion.none").to_string()
                    } else {
                        r.summary.clone()
                    }))
                    .when(!r.follow_up.is_empty(), |d| {
                        d.child(div().text_size(px(12.0)).text_color(c.text2).child(format!(
                            "{} {}",
                            t!("chat.conclusion.follow_up"),
                            r.follow_up
                        )))
                    })
                    .children(r.warnings.iter().map(|w| {
                        div()
                            .text_size(px(12.0))
                            .text_color(c.accent)
                            .child(format!("⚠ {w}"))
                    }))
                    .into_any_element()
            }
            ItemKind::Image { url } => image_el(id, 0, url, &c),
            ItemKind::Attachment { path, mime_type } => h_flex()
                .gap(px(8.0))
                .items_center()
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(c.line)
                .text_size(px(12.0))
                .child(icon(IconName::File).size(px(14.0)))
                .child(path.clone())
                .child(div().text_color(c.text3).child(mime_type.clone()))
                .into_any_element(),
            ItemKind::Json { label, value } => v_flex()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(c.text3)
                        .child(label.to_uppercase()),
                )
                .child(code_box(
                    &serde_json::to_string_pretty(value).unwrap_or_default(),
                    &c,
                ))
                .into_any_element(),
        }
    }

    // ------------------------------------------------------------------ thinking

    fn thinking_el(
        &self,
        id: ItemId,
        text: &str,
        streaming: bool,
        md: &mut HashMap<ItemId, MdEntry>,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = *c;
        let open = self.open.contains(&id);
        let weak = cx.weak_entity();
        let label = if streaming {
            ShimmerText::new(t!("chat.thinking.streaming").to_string())
                .id(SharedString::from(format!("think-shimmer-{}", id.0)))
                .into_any_element()
        } else {
            div()
                .child(t!("chat.thinking.done").to_string())
                .into_any_element()
        };
        v_flex()
            .gap(px(6.0))
            .child(
                h_flex()
                    .id(SharedString::from(format!("think-{}", id.0)))
                    .gap(px(6.0))
                    .items_center()
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .text_color(c.text2)
                    .on_click(move |_, _, cx| {
                        weak.update(cx, |this, cx| this.toggle_open(id, cx)).ok();
                    })
                    .child(
                        icon(if open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(px(13.0)),
                    )
                    .child(label),
            )
            .when(open, |d| {
                let blocks = blocks_for(md, id, text);
                d.child(
                    div()
                        .pl(px(12.0))
                        .border_l_2()
                        .border_color(c.line2)
                        .text_color(c.text2)
                        .child(md_view::blocks_el(&blocks, id.0, &c, BODY - 1.0)),
                )
            })
            .into_any_element()
    }

    pub(super) fn toggle_open(&mut self, id: ItemId, cx: &mut Context<Self>) {
        if !self.open.remove(&id) {
            self.open.insert(id);
        }
        // Opening something must keep it in view, so the tail stops following.
        self.tail = false;
        cx.notify();
    }

    fn toggle_full(&mut self, id: ItemId, cx: &mut Context<Self>) {
        if !self.full.remove(&id) {
            self.full.insert(id);
        }
        cx.notify();
    }

    // ------------------------------------------------------------------ approvals in the flow

    fn approval_row(&self, request_id: &str, c: &Colors) -> AnyElement {
        let c = *c;
        let Some(card) = self.model.approval_card(request_id) else {
            return div().into_any_element();
        };
        use pando_client::approvals::{RequestBody, Resolution};
        let title = match card.request {
            RequestBody::Permission(p) => {
                let what = if p.description.is_empty() {
                    &p.path
                } else {
                    &p.description
                };
                format!("{} {}", p.tool_name, what).trim().to_owned()
            }
            RequestBody::Question(q) => q.questions.first().map_or_else(
                || t!("chat.questions.title").to_string(),
                |q| q.question.clone(),
            ),
        };
        let (state, color) = match (card.pending, card.resolution) {
            (true, _) => (t!("chat.approval.pending").to_string(), c.accent),
            (false, Some(Resolution::Approved)) => {
                (t!("chat.approval.approved").to_string(), c.add_fg)
            }
            (false, Some(Resolution::ApprovedForSession)) => {
                (t!("chat.approval.approved_session").to_string(), c.add_fg)
            }
            (false, Some(Resolution::Denied(r) | Resolution::Cancelled(r))) => (
                t!(
                    "chat.approval.denied",
                    reason = t!(format!("chat.approval.reason.{}", r.as_str()))
                )
                .to_string(),
                c.danger,
            ),
            (false, Some(Resolution::Answered(_))) => {
                (t!("chat.approval.answered").to_string(), c.add_fg)
            }
            (false, None) => (String::new(), c.text3),
        };
        h_flex()
            .gap(px(8.0))
            .items_center()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(if card.pending { c.accent } else { c.line })
            .bg(c.card)
            .text_size(px(12.0))
            .child(
                icon(match card.request {
                    RequestBody::Permission(_) => IconName::TriangleAlert,
                    RequestBody::Question(_) => IconName::Info,
                })
                .size(px(14.0))
                .text_color(color),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(title),
            )
            .child(div().text_color(color).child(state))
            .into_any_element()
    }

    // ------------------------------------------------------------------ activity groups and tool cards

    fn activity_el(&self, ix: &[usize], c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c = *c;
        let items = self.model.items();
        let tools: Vec<(&Item, &ToolItem)> = ix
            .iter()
            .filter_map(|i| items.get(*i))
            .filter_map(|it| match &it.kind {
                ItemKind::Tool(t) => Some((it, t)),
                _ => None,
            })
            .collect();
        if tools.len() == 1 {
            return self.tool_card(tools[0].0.id, tools[0].1, &c, cx);
        }
        let Some((first, _)) = tools.first() else {
            return div().into_any_element();
        };
        // Own key space: the first call's card has its own open state.
        let gid = ItemId(first.id.0 | (1 << 63));
        let refs: Vec<&ToolItem> = tools.iter().map(|(_, t)| *t).collect();
        let (running, failed) = tools::group_state(&refs);
        let open = self.open.contains(&gid);
        let weak = cx.weak_entity();
        let summary = tools::class_counts(&refs)
            .into_iter()
            .map(|(class, n)| class_phrase(class, n))
            .collect::<Vec<_>>()
            .join(" · ");
        let header = h_flex()
            .id(SharedString::from(format!("group-{}", gid.0)))
            .gap(px(8.0))
            .items_center()
            .px(px(10.0))
            .py(px(7.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(|d| d.bg(c.hover))
            .text_size(px(12.0))
            .text_color(c.text2)
            .on_click(move |_, _, cx| {
                weak.update(cx, |this, cx| this.toggle_open(gid, cx)).ok();
            })
            .child(
                icon(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .size(px(13.0)),
            )
            .child(div().flex_1().child(capitalize(&summary)))
            .when(running > 0, |d| {
                d.child(
                    h_flex()
                        .gap(px(5.0))
                        .items_center()
                        .text_color(c.accent)
                        .child(pulse_dot(format!("grp-run-{}", gid.0), c.accent, 6.0))
                        .child(t!("chat.activity.running", count = running).to_string()),
                )
            })
            .when(failed > 0, |d| {
                d.child(
                    div()
                        .text_color(c.danger)
                        .child(t!("chat.activity.failed", count = failed).to_string()),
                )
            });
        v_flex()
            .gap(px(4.0))
            .child(header)
            .when(open, |d| {
                d.child(
                    v_flex()
                        .pl(px(14.0))
                        .gap(px(6.0))
                        .children(tools.iter().map(|(it, t)| self.tool_card(it.id, t, &c, cx))),
                )
            })
            .into_any_element()
    }

    fn tool_card(
        &self,
        id: ItemId,
        tool: &ToolItem,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = *c;
        let class = tools::classify(tool);
        let open = self.open.contains(&id);
        let weak = cx.weak_entity();
        let failed =
            tool.status == ToolStatus::Failed || tool.outcome.as_ref().is_some_and(|o| o.is_error);
        let status: AnyElement = match tool.status {
            ToolStatus::Pending | ToolStatus::Running => {
                pulse_dot(format!("tool-run-{}", id.0), c.accent, 7.0)
            }
            _ if failed => icon(IconName::CircleX)
                .size(px(14.0))
                .text_color(c.danger)
                .into_any_element(),
            _ => icon(IconName::Check)
                .size(px(14.0))
                .text_color(c.add_fg)
                .into_any_element(),
        };
        let header = h_flex()
            .id(SharedString::from(format!("tool-{}", id.0)))
            .gap(px(8.0))
            .items_center()
            .px(px(10.0))
            .py(px(7.0))
            .cursor_pointer()
            .hover(|d| d.bg(c.hover))
            .on_click(move |_, _, cx| {
                weak.update(cx, |this, cx| this.toggle_open(id, cx)).ok();
            })
            .child(status)
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(c.text3)
                    .min_w(px(54.0))
                    .child(class_label(class, tool)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .font_family(crate::fonts::FONT_MONO)
                    .text_size(px(12.0))
                    .child(tools::arg_summary(tool)),
            )
            .children(tool.duration.map(|d| {
                div()
                    .text_size(px(11.0))
                    .text_color(c.text3)
                    .child(tools::duration_text(d))
            }))
            .child(
                icon(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .size(px(13.0))
                .text_color(c.text3),
            );
        v_flex()
            .rounded(px(8.0))
            .border_1()
            .border_color(if failed { c.danger } else { c.line })
            .bg(c.card)
            .child(header)
            .when(open, |d| {
                d.child(
                    v_flex()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .pb(px(10.0))
                        .child(self.tool_body(id, tool, class, &c, cx)),
                )
            })
            .into_any_element()
    }

    /// The renderer of one tool type (PDA-T-0070).
    fn tool_body(
        &self,
        id: ItemId,
        tool: &ToolItem,
        class: ToolClass,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let c = *c;
        let full = self.full.contains(&id);
        let weak = cx.weak_entity();
        let more = move |clipped: &tools::Clipped| -> Option<AnyElement> {
            if !clipped.truncated && !full {
                return None;
            }
            let weak = weak.clone();
            Some(
                Button::new(SharedString::from(format!("more-{}", id.0)))
                    .ghost()
                    .compact()
                    .label(if full {
                        t!("chat.tool.show_less").to_string()
                    } else {
                        t!("chat.tool.show_more", total = clipped.total).to_string()
                    })
                    .on_click(move |_, _, cx| {
                        weak.update(cx, |this, cx| this.toggle_full(id, cx)).ok();
                    })
                    .into_any_element(),
            )
        };
        let full_text = |s: &str, clipped: &tools::Clipped| {
            if full {
                s.to_owned()
            } else {
                clipped.text.clone()
            }
        };
        let content = tool.outcome.as_ref().map_or("", |o| o.content.as_str());
        let mut body = v_flex().gap(px(8.0));
        match class {
            ToolClass::Execute => {
                let v = tools::exec_view(tool);
                body = body
                    .child(
                        div()
                            .font_family(crate::fonts::FONT_MONO)
                            .text_size(px(12.0))
                            .text_color(c.accent)
                            .child(format!("$ {}", v.command)),
                    )
                    .when(!v.output.text.is_empty(), |d| {
                        d.child(code_box(&full_text(content, &v.output), &c))
                    })
                    .children(more(&v.output))
                    .children(v.exit_code.map(|code| {
                        div()
                            .text_size(px(11.0))
                            .text_color(if code == 0 { c.text3 } else { c.danger })
                            .child(t!("chat.tool.exit_code", code = code).to_string())
                    }));
            }
            ToolClass::Edit | ToolClass::Write => {
                if let Some(diff) = tool_diff(tool) {
                    let path = diff.path.clone();
                    let weak = cx.weak_entity();
                    body = body
                        .child(
                            h_flex()
                                .gap(px(10.0))
                                .items_center()
                                .text_size(px(12.0))
                                .child(
                                    div()
                                        .font_family(crate::fonts::FONT_MONO)
                                        .child(path.clone()),
                                )
                                .child(
                                    div()
                                        .text_color(c.add_fg)
                                        .child(format!("+{}", diff.added())),
                                )
                                .child(
                                    div()
                                        .text_color(c.del_fg)
                                        .child(format!("−{}", diff.removed())),
                                )
                                .child(div().flex_1())
                                .child(
                                    Button::new(SharedString::from(format!("open-diff-{}", id.0)))
                                        .ghost()
                                        .compact()
                                        .label(t!("chat.diff.open").to_string())
                                        .on_click(move |_, _, cx| {
                                            let path = path.clone();
                                            weak.update(cx, |this, cx| {
                                                this.diff_file = Some(path);
                                                cx.notify();
                                            })
                                            .ok();
                                        }),
                                ),
                        )
                        .child(diff_lines_el(&diff.lines, DIFF_PREVIEW_LINES, &c));
                } else {
                    body = body.child(self.json_body(tool, &c));
                }
            }
            ToolClass::Read => {
                let v = tools::read_view(tool);
                body = body
                    .child(
                        div()
                            .font_family(crate::fonts::FONT_MONO)
                            .text_size(px(12.0))
                            .text_color(c.text2)
                            .child(v.path.clone()),
                    )
                    .when(!v.content.text.is_empty(), |d| {
                        d.child(code_box(&full_text(content, &v.content), &c))
                    })
                    .children(more(&v.content));
            }
            ToolClass::Search => {
                let v = tools::search_view(tool);
                body = body
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .child(
                                div()
                                    .font_family(crate::fonts::FONT_MONO)
                                    .text_color(c.accent)
                                    .child(v.query.clone()),
                            )
                            .child(
                                div()
                                    .text_color(c.text3)
                                    .child(t!("chat.tool.results", count = v.total).to_string()),
                            ),
                    )
                    .children(v.lines.iter().map(|l| {
                        div()
                            .font_family(crate::fonts::FONT_MONO)
                            .text_size(px(11.0))
                            .text_color(c.text2)
                            .child(l.clone())
                    }))
                    .when(v.total > v.lines.len(), |d| {
                        d.child(
                            div().text_size(px(11.0)).text_color(c.text3).child(
                                t!("chat.tool.more_results", count = v.total - v.lines.len())
                                    .to_string(),
                            ),
                        )
                    });
            }
            ToolClass::Fetch => {
                let v = tools::fetch_view(tool);
                body = body
                    .child(
                        div()
                            .font_family(crate::fonts::FONT_MONO)
                            .text_size(px(12.0))
                            .text_color(c.info)
                            .child(v.url.clone()),
                    )
                    .when(!v.summary.text.is_empty(), |d| {
                        d.child(code_box(&full_text(content, &v.summary), &c))
                    })
                    .children(more(&v.summary));
            }
            ToolClass::Think | ToolClass::Other => {
                body = body.child(self.json_body(tool, &c));
            }
        }
        for (n, url) in tools::images(tool).iter().enumerate() {
            body = body.child(image_el(id, n + 1, url, &c));
        }
        body.into_any_element()
    }

    fn json_body(&self, tool: &ToolItem, c: &Colors) -> AnyElement {
        let v = tools::json_view(tool);
        v_flex()
            .gap(px(6.0))
            .child(label_el(&t!("chat.tool.input"), c))
            .child(code_box(&v.input.text, c))
            .when(!v.output.text.is_empty(), |d| {
                d.child(label_el(&t!("chat.tool.output"), c))
                    .child(code_box(&v.output.text, c))
            })
            .into_any_element()
    }
}

fn label_el(text: &str, c: &Colors) -> AnyElement {
    div()
        .text_size(px(10.0))
        .text_color(c.text3)
        .child(text.to_uppercase())
        .into_any_element()
}

/// Monospace block for output and JSON.
pub(super) fn code_box(text: &str, c: &Colors) -> AnyElement {
    div()
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(6.0))
        .bg(c.deep)
        .border_1()
        .border_color(c.line)
        .font_family(crate::fonts::FONT_MONO)
        .text_size(px(11.5))
        .text_color(c.text)
        .child(text.to_owned())
        .into_any_element()
}

/// Coloured diff lines (at most `max`).
pub(super) fn diff_lines_el(lines: &[super::diff::DiffLine], max: usize, c: &Colors) -> AnyElement {
    let shown = lines.len().min(max);
    v_flex()
        .rounded(px(6.0))
        .border_1()
        .border_color(c.line)
        .bg(c.deep)
        .overflow_hidden()
        .children(lines.iter().take(shown).map(|l| {
            let (mark, fg, bg) = match l.kind {
                LineKind::Added => ("+", c.add_fg, Some(c.add_bg)),
                LineKind::Removed => ("−", c.del_fg, Some(c.del_bg)),
                LineKind::Same => (" ", c.text2, None),
            };
            h_flex()
                .px(px(8.0))
                .gap(px(8.0))
                .font_family(crate::fonts::FONT_MONO)
                .text_size(px(11.5))
                .text_color(fg)
                .when_some(bg, |d, bg| d.bg(bg))
                .child(div().w(px(10.0)).child(mark))
                .child(div().flex_1().min_w_0().child(l.text.clone()))
        }))
        .when(lines.len() > shown, |d| {
            d.child(
                div()
                    .px(px(8.0))
                    .py(px(3.0))
                    .text_size(px(11.0))
                    .text_color(c.text3)
                    .child(t!("chat.diff.more_lines", count = lines.len() - shown).to_string()),
            )
        })
        .into_any_element()
}

fn image_el(id: ItemId, n: usize, url: &str, c: &Colors) -> AnyElement {
    let decoded = tools::decode_data_url(url);
    match decoded {
        Some((kind, bytes)) => {
            let format = match kind {
                tools::ImageKind::Png => ImageFormat::Png,
                tools::ImageKind::Jpeg => ImageFormat::Jpeg,
                tools::ImageKind::Gif => ImageFormat::Gif,
                tools::ImageKind::Webp => ImageFormat::Webp,
            };
            let _ = (id, n);
            img(Arc::new(Image::from_bytes(format, bytes)))
                .max_h(px(260.0))
                .max_w(px(420.0))
                .rounded(px(8.0))
                .object_fit(ObjectFit::Contain)
                .into_any_element()
        }
        None => div()
            .text_size(px(11.0))
            .text_color(c.text3)
            .child(t!("chat.tool.image_unavailable").to_string())
            .into_any_element(),
    }
}

fn notice_el(kind: &NoticeKind, text: &str, c: &Colors) -> AnyElement {
    let (name, color, label) = match kind {
        NoticeKind::Routing => (IconName::Network, c.info, t!("chat.notice.routing")),
        NoticeKind::System => (IconName::Info, c.text3, t!("chat.notice.system")),
        NoticeKind::Resurrected => (IconName::RotateCw, c.accent, t!("chat.notice.resurrected")),
        NoticeKind::ConclusionQueued => {
            (IconName::Inbox, c.info, t!("chat.notice.conclusion_queued"))
        }
        NoticeKind::ConclusionInjected => (
            IconName::Inbox,
            c.info,
            t!("chat.notice.conclusion_injected"),
        ),
        NoticeKind::Cancelled => (IconName::Ban, c.text3, t!("chat.notice.cancelled")),
        NoticeKind::MaxTokens => (
            IconName::TriangleAlert,
            c.accent,
            t!("chat.notice.max_tokens"),
        ),
        NoticeKind::PermissionDenied => {
            (IconName::Ban, c.danger, t!("chat.notice.permission_denied"))
        }
        NoticeKind::Error => (IconName::CircleAlert, c.danger, t!("chat.notice.error")),
    };
    let body: String = text.trim().to_owned();
    h_flex()
        .gap(px(8.0))
        .items_start()
        .px(px(10.0))
        .py(px(6.0))
        .rounded(px(8.0))
        .border_l_2()
        .border_color(color)
        .bg(c.card)
        .text_size(px(12.0))
        .child(icon(name).size(px(14.0)).text_color(color).mt(px(1.0)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(c.text2)
                .child(
                    div()
                        .text_color(color)
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_string()),
                )
                .when(!body.is_empty(), |d| d.child(body)),
        )
        .into_any_element()
}

/// "Ejecutó 3 comandos", "Leyó 1 fichero".
pub(super) fn class_phrase(class: ToolClass, n: usize) -> String {
    let key = if n == 1 { "one" } else { "other" };
    t!(format!("chat.activity.{}_{key}", class.key()), count = n).to_string()
}

fn class_label(class: ToolClass, tool: &ToolItem) -> String {
    let base = t!(format!("chat.tool.class.{}", class.key())).to_string();
    if class == ToolClass::Other && !tool.name.is_empty() {
        tool.name.clone()
    } else {
        base
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + chars.as_str()
    })
}
