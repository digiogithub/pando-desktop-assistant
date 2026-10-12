//! Banners above the transcript (plan, todos, goal), the usage panel, the changed-files bar and
//! the diff viewer (PDA-T-0071, 0077, 0078).

use pando_client::model::GoalBanner;
use pando_client::steering::StopPlan;
use rust_i18n::t;

use super::composer::compact_tokens;
use super::diff::{self, LineKind};
use super::panel::ChatPanel;
use super::render_transcript::diff_lines_el;
use super::style::Colors;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::text_edit::FontWeight;
use crate::ui::{
    AnyElement, Context, FluentBuilder as _, Hsla, IconName, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, div, h_flex,
    icon, px, v_flex,
};

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn status_color(status: &str, c: &Colors) -> Hsla {
    match status {
        "completed" => c.add_fg,
        "in_progress" | "running" => c.accent,
        "failed" | "stalled" | "timeout" | "blocked" => c.danger,
        _ => c.text3,
    }
}

impl ChatPanel {
    // ------------------------------------------------------------------ banners

    pub(super) fn banners(&mut self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c = *c;
        let goal = self.model.goal_banner();
        let plan_n = self.model.plan().len();
        let todos_n = self.model.todos().len();
        if goal.is_none() && plan_n == 0 && todos_n == 0 {
            return div().into_any_element();
        }
        v_flex()
            .flex_shrink_0()
            .px(px(16.0))
            .pt(px(8.0))
            .gap(px(6.0))
            .children(goal.map(|g| self.goal_banner(&g, &c, cx)))
            .when(plan_n > 0, |d| d.child(self.plan_banner(&c, cx)))
            .when(todos_n > 0, |d| d.child(self.todos_banner(&c, cx)))
            .into_any_element()
    }

    fn goal_banner(&self, g: &GoalBanner, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c = *c;
        let weak = cx.weak_entity();
        let color = status_color(&g.status, &c);
        v_flex()
            .gap(px(4.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(color)
            .bg(c.card)
            .child(
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .text_size(px(12.0))
                    .child(icon(IconName::Star).size(px(14.0)).text_color(color))
                    .child(
                        div()
                            .text_color(c.text3)
                            .child(t!("chat.goal.label").to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .font_weight(FontWeight::MEDIUM)
                            .child(g.objective.clone()),
                    )
                    .child(
                        div()
                            .px(px(8.0))
                            .rounded_full()
                            .border_1()
                            .border_color(color)
                            .text_color(color)
                            .text_size(px(10.0))
                            .child(t!(format!("chat.goal.status.{}", g.status)).to_string()),
                    )
                    .child(
                        div()
                            .text_color(c.text3)
                            .child(format!("{}/{}", g.iteration, g.max_iterations)),
                    )
                    .when(g.can_cancel, |d| {
                        d.child(
                            Button::new("goal-cancel")
                                .ghost()
                                .compact()
                                .label(t!("chat.goal.cancel").to_string())
                                .on_click(move |_, _, cx| {
                                    weak.update(cx, |this, cx| {
                                        this.stop_with(
                                            StopPlan {
                                                cancel_run: false,
                                                cancel_goal: true,
                                            },
                                            cx,
                                        );
                                    })
                                    .ok();
                                }),
                        )
                    }),
            )
            .children(
                g.progress
                    .clone()
                    .map(|p| div().text_size(px(12.0)).text_color(c.text2).child(p)),
            )
            .children(g.next_step.clone().map(|n| {
                div()
                    .text_size(px(12.0))
                    .text_color(c.text3)
                    .child(format!("{} {n}", t!("chat.goal.next")))
            }))
            .into_any_element()
    }

    fn collapsible(
        &self,
        key: &'static str,
        title: String,
        summary: String,
        c: &Colors,
        cx: &mut Context<Self>,
        body: impl IntoElement,
    ) -> AnyElement {
        let c = *c;
        let open = self.banners_open.contains(key);
        let weak = cx.weak_entity();
        v_flex()
            .rounded(px(10.0))
            .border_1()
            .border_color(c.line)
            .bg(c.card)
            .child(
                h_flex()
                    .id(SharedString::from(format!("banner-{key}")))
                    .gap(px(8.0))
                    .items_center()
                    .px(px(12.0))
                    .py(px(6.0))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .on_click(move |_, _, cx| {
                        weak.update(cx, |this, cx| {
                            if !this.banners_open.remove(key) {
                                this.banners_open.insert(key);
                            }
                            cx.notify();
                        })
                        .ok();
                    })
                    .child(
                        icon(if open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(px(13.0)),
                    )
                    .child(div().font_weight(FontWeight::MEDIUM).child(title))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(c.text2)
                            .child(summary),
                    ),
            )
            .when(open, |d| {
                d.child(div().px(px(12.0)).pb(px(8.0)).child(body))
            })
            .into_any_element()
    }

    fn plan_banner(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c2 = *c;
        let plan = self.model.plan();
        let done = plan.iter().filter(|e| e.status == "completed").count();
        let active = plan
            .iter()
            .find(|e| e.status == "in_progress")
            .map(|e| {
                e.active_form
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| e.title.clone())
            })
            .unwrap_or_default();
        let body = v_flex().gap(px(3.0)).children(plan.iter().map(|e| {
            let color = status_color(&e.status, &c2);
            h_flex()
                .gap(px(8.0))
                .items_center()
                .text_size(px(12.0))
                .child(
                    icon(match e.status.as_str() {
                        "completed" => IconName::CircleCheck,
                        "in_progress" => IconName::Loader,
                        _ => IconName::Square,
                    })
                    .size(px(13.0))
                    .text_color(color),
                )
                .child(
                    div()
                        .text_color(if e.status == "completed" {
                            c2.text3
                        } else {
                            c2.text
                        })
                        .child(e.title.clone()),
                )
        }));
        self.collapsible(
            "plan",
            t!("chat.plan.title").to_string(),
            format!("{done}/{} {active}", plan.len()),
            c,
            cx,
            body,
        )
    }

    fn todos_banner(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c2 = *c;
        let todos = self.model.todos();
        let done = todos.iter().filter(|e| e.status == "completed").count();
        let body = v_flex().gap(px(3.0)).children(todos.iter().map(|e| {
            let color = status_color(&e.status, &c2);
            h_flex()
                .gap(px(8.0))
                .items_center()
                .text_size(px(12.0))
                .child(
                    icon(if e.status == "completed" {
                        IconName::CircleCheck
                    } else {
                        IconName::Square
                    })
                    .size(px(13.0))
                    .text_color(color),
                )
                .child(div().flex_1().child(e.content.clone()))
                .when(!e.priority.is_empty(), |d| {
                    d.child(
                        div()
                            .px(px(6.0))
                            .rounded_full()
                            .border_1()
                            .border_color(c2.line2)
                            .text_size(px(9.0))
                            .text_color(if e.priority == "high" {
                                c2.accent
                            } else {
                                c2.text3
                            })
                            .child(e.priority.clone()),
                    )
                })
        }));
        self.collapsible(
            "todos",
            t!("chat.todos.title").to_string(),
            format!("{done}/{}", todos.len()),
            c,
            cx,
            body,
        )
    }

    // ------------------------------------------------------------------ usage

    pub(super) fn usage_chip(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c = *c;
        let Some(u) = self.model.usage().current() else {
            return div().into_any_element();
        };
        let estimated = self.model.usage().is_estimated();
        let weak = cx.weak_entity();
        let total =
            u.prompt_tokens + u.completion_tokens + u.cache_read_tokens + u.cache_creation_tokens;
        let chip = h_flex()
            .id("chat-usage-chip")
            .gap(px(6.0))
            .items_center()
            .px(px(8.0))
            .py(px(2.0))
            .rounded_full()
            .border_1()
            .border_color(if self.usage_open { c.accent } else { c.line2 })
            .cursor_pointer()
            .hover(|d| d.bg(c.hover))
            .text_size(px(11.0))
            .text_color(c.text2)
            .on_click(move |_, _, cx| {
                weak.update(cx, |this, cx| {
                    this.usage_open = !this.usage_open;
                    cx.notify();
                })
                .ok();
            })
            .child(icon(IconName::ChartPie).size(px(12.0)))
            .child(format!(
                "{}{} · ${:.4}",
                if estimated { "~" } else { "" },
                compact_tokens(total),
                u.cost
            ));
        // Deferred: painted above the banners that follow the header in the tree.
        let panel = self
            .usage_open
            .then(|| crate::ui::deferred(self.usage_panel(&c)).with_priority(1));
        div()
            .relative()
            .child(chip)
            .children(panel)
            .into_any_element()
    }

    fn usage_panel(&self, c: &Colors) -> AnyElement {
        let c = *c;
        let usage = self.model.usage();
        let Some(u) = usage.current() else {
            return div().into_any_element();
        };
        let row = |label: String, value: String| {
            h_flex()
                .justify_between()
                .gap(px(24.0))
                .child(div().text_color(c.text2).child(label))
                .child(div().font_family(crate::fonts::FONT_MONO).child(value))
        };
        let tok = |n: i64| {
            if usage.is_estimated() {
                format!("~{n}")
            } else {
                n.to_string()
            }
        };
        let totals = usage.totals();
        v_flex()
            .id("chat-usage-panel")
            .absolute()
            .top(px(30.0))
            .right(px(0.0))
            .w(px(280.0))
            .gap(px(4.0))
            .p(px(12.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(c.line2)
            .bg(c.bg)
            .shadow_lg()
            .text_size(px(12.0))
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(t!("chat.usage.title").to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(if usage.is_estimated() {
                                c.accent
                            } else {
                                c.add_fg
                            })
                            .child(if usage.is_estimated() {
                                t!("chat.usage.estimated").to_string()
                            } else {
                                t!("chat.usage.confirmed").to_string()
                            }),
                    ),
            )
            .child(row(
                t!("chat.usage.prompt").to_string(),
                tok(u.prompt_tokens),
            ))
            .child(row(
                t!("chat.usage.completion").to_string(),
                tok(u.completion_tokens),
            ))
            .child(row(
                t!("chat.usage.cache_read").to_string(),
                tok(u.cache_read_tokens),
            ))
            .child(row(
                t!("chat.usage.cache_creation").to_string(),
                tok(u.cache_creation_tokens),
            ))
            .child(row(
                t!("chat.usage.reasoning").to_string(),
                tok(u.reasoning_tokens),
            ))
            .child(row(
                t!("chat.usage.cost").to_string(),
                format!("${:.4}", u.cost),
            ))
            .when(u.context_window > 0, |d| {
                d.child(row(
                    t!("chat.usage.window").to_string(),
                    compact_tokens(u.context_window),
                ))
            })
            .when(usage.is_estimated() && totals.is_some(), |d| {
                let t = totals.unwrap_or_default();
                d.child(
                    div()
                        .pt(px(4.0))
                        .border_t_1()
                        .border_color(c.line)
                        .text_color(c.text3)
                        .child(format!(
                            "{} {} · ${:.4}",
                            t!("chat.usage.confirmed_so_far"),
                            compact_tokens(
                                t.prompt_tokens
                                    + t.completion_tokens
                                    + t.cache_read_tokens
                                    + t.cache_creation_tokens
                            ),
                            t.cost
                        )),
                )
            })
            .into_any_element()
    }

    // ------------------------------------------------------------------ changed files

    pub(super) fn changed_files_bar(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let c = *c;
        let files = diff::changed_files_of(self.model.items());
        if files.is_empty() {
            return div().into_any_element();
        }
        let (n, added, removed) = diff::totals(&files);
        const SHOWN: usize = 6;
        h_flex()
            .flex_shrink_0()
            .flex_wrap()
            .gap(px(6.0))
            .items_center()
            .px(px(16.0))
            .pb(px(6.0))
            .text_size(px(11.0))
            .child(div().text_color(c.text2).child(if n == 1 {
                t!("chat.files.summary_one").to_string()
            } else {
                t!("chat.files.summary_other", count = n).to_string()
            }))
            .child(div().text_color(c.add_fg).child(format!("+{added}")))
            .child(div().text_color(c.del_fg).child(format!("−{removed}")))
            .children(files.iter().take(SHOWN).map(|f| {
                let weak = cx.weak_entity();
                let path = f.path.clone();
                h_flex()
                    .id(SharedString::from(format!("file-{}", f.path)))
                    .gap(px(6.0))
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded_full()
                    .border_1()
                    .border_color(c.line2)
                    .cursor_pointer()
                    .hover(|d| d.bg(c.hover))
                    .on_click(move |_, _, cx| {
                        let path = path.clone();
                        weak.update(cx, |this, cx| {
                            this.diff_file = Some(path);
                            cx.notify();
                        })
                        .ok();
                    })
                    .child(basename(&f.path).to_owned())
                    .child(div().text_color(c.add_fg).child(format!("+{}", f.added)))
                    .child(div().text_color(c.del_fg).child(format!("−{}", f.removed)))
            }))
            .when(files.len() > SHOWN, |d| {
                d.child(
                    div()
                        .text_color(c.text3)
                        .child(format!("+{}", files.len() - SHOWN)),
                )
            })
            .into_any_element()
    }

    // ------------------------------------------------------------------ diff viewer

    pub(super) fn diff_viewer(&self, c: &Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let path = self.diff_file.as_ref()?;
        let c = *c;
        let files = diff::changed_files_of(self.model.items());
        let file = files.iter().find(|f| &f.path == path)?;
        let weak = cx.weak_entity();
        let (w_close, w_split) = (weak.clone(), weak);
        let split = self.diff_split;
        let body = v_flex().gap(px(12.0)).children(file.diffs.iter().map(|d| {
            if split {
                side_by_side_el(&d.lines, &c)
            } else {
                diff_lines_el(&d.lines, usize::MAX, &c)
            }
        }));
        Some(
            div()
                .id("chat-diff-viewer")
                .occlude()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
                .bg(Hsla { a: 0.55, ..c.deep })
                .p(px(24.0))
                .child(
                    v_flex()
                        .size_full()
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(c.line2)
                        .bg(c.bg)
                        .child(
                            h_flex()
                                .gap(px(10.0))
                                .items_center()
                                .px(px(14.0))
                                .py(px(10.0))
                                .border_b_1()
                                .border_color(c.line)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .font_family(crate::fonts::FONT_MONO)
                                        .text_size(px(12.0))
                                        .child(file.path.clone()),
                                )
                                .child(div().text_color(c.add_fg).child(format!("+{}", file.added)))
                                .child(
                                    div()
                                        .text_color(c.del_fg)
                                        .child(format!("−{}", file.removed)),
                                )
                                .child(
                                    Button::new("diff-split")
                                        .ghost()
                                        .compact()
                                        .label(if split {
                                            t!("chat.diff.unified").to_string()
                                        } else {
                                            t!("chat.diff.split").to_string()
                                        })
                                        .on_click(move |_, _, cx| {
                                            w_split
                                                .update(cx, |this, cx| {
                                                    this.diff_split = !this.diff_split;
                                                    cx.notify();
                                                })
                                                .ok();
                                        }),
                                )
                                .child(
                                    Button::new("diff-close")
                                        .ghost()
                                        .compact()
                                        .icon(IconName::Close)
                                        .on_click(move |_, _, cx| {
                                            w_close
                                                .update(cx, |this, cx| {
                                                    this.diff_file = None;
                                                    cx.notify();
                                                })
                                                .ok();
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .id("chat-diff-body")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .p(px(14.0))
                                .child(body),
                        ),
                )
                .into_any_element(),
        )
    }
}

fn side_by_side_el(lines: &[diff::DiffLine], c: &Colors) -> AnyElement {
    let c = *c;
    let cell = |l: &Option<diff::DiffLine>| {
        let (fg, bg) = match l.as_ref().map(|l| l.kind) {
            Some(LineKind::Added) => (c.add_fg, Some(c.add_bg)),
            Some(LineKind::Removed) => (c.del_fg, Some(c.del_bg)),
            _ => (c.text2, None),
        };
        div()
            .flex_1()
            .min_w_0()
            .px(px(8.0))
            .font_family(crate::fonts::FONT_MONO)
            .text_size(px(11.5))
            .text_color(fg)
            .when_some(bg, |d, bg| d.bg(bg))
            .child(l.as_ref().map_or_else(String::new, |l| l.text.clone()))
    };
    v_flex()
        .rounded(px(6.0))
        .border_1()
        .border_color(c.line)
        .bg(c.deep)
        .overflow_hidden()
        .children(diff::side_by_side(lines).into_iter().map(|(l, r)| {
            h_flex()
                .child(cell(&l))
                .child(div().w(px(1.0)).bg(c.line))
                .child(cell(&r))
        }))
        .into_any_element()
}
