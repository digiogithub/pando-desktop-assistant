//! Renders parsed markdown (`markdown::Block`) as GPUI elements (PDA-T-0066).

use std::ops::Range;

use rust_i18n::t;

use super::markdown::{Block, Inline};
use super::style::Colors;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::text_edit::{
    ClipboardItem, FontStyle, FontWeight, HighlightStyle, InteractiveText, StrikethroughStyle,
    StyledText, UnderlineStyle,
};
use crate::ui::{
    AnyElement, FluentBuilder as _, IntoElement, ParentElement as _, SharedString, Styled as _,
    div, h_flex, px, v_flex,
};

/// Base text size of an answer.
pub const BODY: f32 = 14.0;

struct Ctx<'a> {
    c: &'a Colors,
    key: u64,
    next: usize,
    size: f32,
}

impl Ctx<'_> {
    fn id(&mut self, what: &str) -> SharedString {
        self.next += 1;
        format!("md-{}-{what}-{}", self.key, self.next).into()
    }
}

/// Renders `blocks`; `key` keeps element ids unique per transcript item.
#[must_use]
pub fn blocks_el(blocks: &[Block], key: u64, c: &Colors, size: f32) -> AnyElement {
    let mut cx = Ctx {
        c,
        key,
        next: 0,
        size,
    };
    v_flex()
        .gap(px(size * 0.6))
        .children(blocks.iter().map(|b| block_el(b, &mut cx)))
        .into_any_element()
}

fn block_el(block: &Block, cx: &mut Ctx<'_>) -> AnyElement {
    let c = *cx.c;
    match block {
        Block::Paragraph(i) => inline_el(i, cx, cx.size),
        Block::Heading { level, text } => {
            let size = match level {
                1 => cx.size * 1.35,
                2 => cx.size * 1.2,
                3 => cx.size * 1.08,
                _ => cx.size,
            };
            div()
                .mt(px(cx.size * 0.3))
                .font_weight(FontWeight::SEMIBOLD)
                .child(inline_el(text, cx, size))
                .into_any_element()
        }
        Block::List {
            ordered,
            start,
            items,
        } => v_flex()
            .gap(px(cx.size * 0.3))
            .pl(px(2.0))
            .children(items.iter().enumerate().map(|(n, item)| {
                let marker = if *ordered {
                    format!("{}.", start + n as u64)
                } else {
                    "•".to_owned()
                };
                h_flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w(px(18.0))
                            .text_color(c.text2)
                            .text_size(px(cx.size))
                            .child(marker),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(cx.size * 0.3))
                            .children(item.iter().map(|b| block_el(b, cx))),
                    )
            }))
            .into_any_element(),
        Block::Quote(inner) => div()
            .border_l_2()
            .border_color(c.line2)
            .pl(px(12.0))
            .text_color(c.text2)
            .child(
                v_flex()
                    .gap(px(cx.size * 0.4))
                    .children(inner.iter().map(|b| block_el(b, cx))),
            )
            .into_any_element(),
        Block::Code { lang, text } => code_block(lang, text, cx),
        Block::Rule => div().h(px(1.0)).w_full().bg(c.line2).into_any_element(),
        Block::Table { head, rows, right } => {
            let cell = |i: &Inline, col: usize, bold: bool, cx: &mut Ctx<'_>| {
                let right_aligned = right.get(col).copied().unwrap_or(false);
                div()
                    .flex_1()
                    .min_w_0()
                    .px(px(8.0))
                    .py(px(4.0))
                    .when(right_aligned, |d| d.text_right())
                    .when(bold, |d| d.font_weight(FontWeight::SEMIBOLD))
                    .child(inline_el(i, cx, cx.size - 1.0))
            };
            v_flex()
                .border_1()
                .border_color(c.line2)
                .rounded(px(6.0))
                .child(
                    h_flex()
                        .bg(c.card)
                        .children(head.iter().enumerate().map(|(i, h)| cell(h, i, true, cx))),
                )
                .children(rows.iter().map(|r| {
                    h_flex()
                        .border_t_1()
                        .border_color(c.line)
                        .children(r.iter().enumerate().map(|(i, h)| cell(h, i, false, cx)))
                }))
                .into_any_element()
        }
    }
}

fn code_block(lang: &str, text: &str, cx: &mut Ctx<'_>) -> AnyElement {
    let c = *cx.c;
    let copy_id = cx.id("copy");
    let body = text.to_owned();
    let label = if lang.is_empty() {
        t!("chat.code.label").to_string()
    } else {
        lang.to_owned()
    };
    v_flex()
        .rounded(px(8.0))
        .border_1()
        .border_color(c.line)
        .bg(c.deep)
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .px(px(10.0))
                .py(px(2.0))
                .border_b_1()
                .border_color(c.line)
                .child(div().text_size(px(11.0)).text_color(c.text3).child(label))
                .child(
                    Button::new(copy_id)
                        .ghost()
                        .compact()
                        .label(t!("chat.code.copy").to_string())
                        .on_click(move |_, _, app| {
                            app.write_to_clipboard(ClipboardItem::new_string(body.clone()));
                        }),
                ),
        )
        .child(
            div()
                .px(px(12.0))
                .py(px(8.0))
                .font_family(crate::fonts::FONT_MONO)
                .text_size(px(cx.size - 1.5))
                .text_color(c.text)
                .child(text.to_owned()),
        )
        .into_any_element()
}

/// Highlights of an inline run.
type Highlights = Vec<(Range<usize>, HighlightStyle)>;
type Links = Vec<(Range<usize>, String)>;

fn highlights(i: &Inline, c: &Colors) -> (Highlights, Links) {
    let mut hl = Vec::new();
    let mut links = Vec::new();
    for (range, style) in &i.spans {
        let mut h = HighlightStyle::default();
        if style.bold {
            h.font_weight = Some(FontWeight::BOLD);
        }
        if style.italic {
            h.font_style = Some(FontStyle::Italic);
        }
        if style.strike {
            h.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.0),
                color: Some(c.text2),
            });
        }
        if style.code {
            h.background_color = Some(c.card);
            h.color = Some(c.accent);
        }
        if let Some(url) = &style.link {
            h.color = Some(c.info);
            h.underline = Some(UnderlineStyle {
                thickness: px(1.0),
                color: Some(c.info),
                wavy: false,
            });
            links.push((range.clone(), url.clone()));
        }
        hl.push((range.clone(), h));
    }
    (hl, links)
}

fn inline_el(i: &Inline, cx: &mut Ctx<'_>, size: f32) -> AnyElement {
    let (hl, links) = highlights(i, cx.c);
    let styled = StyledText::new(i.text.clone()).with_highlights(hl);
    let el = if links.is_empty() {
        styled.into_any_element()
    } else {
        let ranges: Vec<Range<usize>> = links.iter().map(|(r, _)| r.clone()).collect();
        let urls: Vec<String> = links.into_iter().map(|(_, u)| u).collect();
        InteractiveText::new(cx.id("link"), styled)
            .on_click(ranges, move |ix, _, app| {
                if let Some(url) = urls.get(ix)
                    && (url.starts_with("http://") || url.starts_with("https://"))
                {
                    app.open_url(url);
                }
            })
            .into_any_element()
    };
    div()
        .text_size(px(size))
        .line_height(px(size * 1.55))
        .child(el)
        .into_any_element()
}
