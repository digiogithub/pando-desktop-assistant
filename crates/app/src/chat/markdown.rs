//! Markdown of assistant answers (PDA-T-0066): a pure parser from text to renderable blocks.
//!
//! Parsing is cheap enough to rerun on every streamed delta (the view caches by text length).
//! An unclosed code fence is shown as code up to the end of the text, so a half-streamed answer
//! never flickers into prose: `pulldown-cmark` already treats an unterminated fence that way.

use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Style of one inline run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpanStyle {
    /// Strong emphasis.
    pub bold: bool,
    /// Emphasis.
    pub italic: bool,
    /// Strikethrough.
    pub strike: bool,
    /// Inline code.
    pub code: bool,
    /// Link target.
    pub link: Option<String>,
}

impl SpanStyle {
    fn is_plain(&self) -> bool {
        *self == Self::default()
    }
}

/// Text with styled byte ranges.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inline {
    /// The text, without markup.
    pub text: String,
    /// Non-overlapping styled ranges in ascending order.
    pub spans: Vec<(Range<usize>, SpanStyle)>,
}

impl Inline {
    fn push(&mut self, s: &str, style: &SpanStyle) {
        if s.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(s);
        if !style.is_plain() {
            match self.spans.last_mut() {
                Some((range, last)) if range.end == start && last == style => {
                    range.end = self.text.len();
                }
                _ => self.spans.push((start..self.text.len(), style.clone())),
            }
        }
    }

    /// Nothing to show.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// A block of an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// `#`..`######`.
    Heading {
        /// 1..=6.
        level: u8,
        /// Content.
        text: Inline,
    },
    /// A paragraph.
    Paragraph(Inline),
    /// A list.
    List {
        /// Numbered.
        ordered: bool,
        /// First number of an ordered list.
        start: u64,
        /// Items, each a sequence of blocks.
        items: Vec<Vec<Block>>,
    },
    /// A block quote.
    Quote(Vec<Block>),
    /// A fenced or indented code block.
    Code {
        /// Language tag (may be empty).
        lang: String,
        /// Code without the trailing newline.
        text: String,
    },
    /// A table.
    Table {
        /// Header cells.
        head: Vec<Inline>,
        /// Body rows.
        rows: Vec<Vec<Inline>>,
        /// Column alignments (`true` = right aligned).
        right: Vec<bool>,
    },
    /// A thematic break.
    Rule,
}

type TableAcc = (Vec<Inline>, Vec<Vec<Inline>>, Vec<bool>);

enum Frame {
    Doc(Vec<Block>),
    Quote(Vec<Block>),
    Item(Vec<Block>),
    List {
        ordered: bool,
        start: u64,
        items: Vec<Vec<Block>>,
    },
}

/// Parses `text` into blocks.
#[must_use]
pub fn parse(text: &str) -> Vec<Block> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let mut stack = vec![Frame::Doc(Vec::new())];
    let mut inline = Inline::default();
    let mut style = SpanStyle::default();
    let mut styles: Vec<SpanStyle> = Vec::new();
    let mut heading: Option<u8> = None;
    let mut code: Option<(String, String)> = None;
    let mut table: Option<TableAcc> = None;
    let mut row: Vec<Inline> = Vec::new();
    let mut in_head = false;

    fn emit(stack: &mut [Frame], block: Block) {
        if let Some(Frame::Doc(v) | Frame::Quote(v) | Frame::Item(v)) = stack.last_mut() {
            v.push(block);
        }
    }
    fn flush_inline(stack: &mut [Frame], inline: &mut Inline) {
        // Loose text of a tight list item has no paragraph around it.
        if !inline.is_empty() {
            emit(stack, Block::Paragraph(std::mem::take(inline)));
        }
    }

    for ev in Parser::new_ext(text, opts) {
        match ev {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {}
                Tag::Heading { level, .. } => {
                    flush_inline(&mut stack, &mut inline);
                    heading = Some(match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    });
                }
                Tag::BlockQuote(_) => {
                    flush_inline(&mut stack, &mut inline);
                    stack.push(Frame::Quote(Vec::new()));
                }
                Tag::CodeBlock(kind) => {
                    flush_inline(&mut stack, &mut inline);
                    let lang = match kind {
                        CodeBlockKind::Fenced(l) => {
                            l.split_whitespace().next().unwrap_or("").to_owned()
                        }
                        CodeBlockKind::Indented => String::new(),
                    };
                    code = Some((lang, String::new()));
                }
                Tag::List(start) => {
                    flush_inline(&mut stack, &mut inline);
                    stack.push(Frame::List {
                        ordered: start.is_some(),
                        start: start.unwrap_or(1),
                        items: Vec::new(),
                    });
                }
                Tag::Item => stack.push(Frame::Item(Vec::new())),
                Tag::Table(aligns) => {
                    table = Some((
                        Vec::new(),
                        Vec::new(),
                        aligns.iter().map(|a| *a == Alignment::Right).collect(),
                    ));
                }
                Tag::TableHead => in_head = true,
                Tag::TableRow | Tag::TableCell => {}
                Tag::Emphasis => {
                    styles.push(style.clone());
                    style.italic = true;
                }
                Tag::Strong => {
                    styles.push(style.clone());
                    style.bold = true;
                }
                Tag::Strikethrough => {
                    styles.push(style.clone());
                    style.strike = true;
                }
                Tag::Link { dest_url, .. } => {
                    styles.push(style.clone());
                    style.link = Some(dest_url.to_string());
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => flush_inline(&mut stack, &mut inline),
                TagEnd::Heading(_) => {
                    let level = heading.take().unwrap_or(1);
                    emit(
                        &mut stack,
                        Block::Heading {
                            level,
                            text: std::mem::take(&mut inline),
                        },
                    );
                }
                TagEnd::BlockQuote(_) => {
                    flush_inline(&mut stack, &mut inline);
                    if let Some(Frame::Quote(v)) = stack.pop() {
                        emit(&mut stack, Block::Quote(v));
                    }
                }
                TagEnd::CodeBlock => {
                    if let Some((lang, mut body)) = code.take() {
                        if body.ends_with('\n') {
                            body.pop();
                        }
                        emit(&mut stack, Block::Code { lang, text: body });
                    }
                }
                TagEnd::List(_) => {
                    if let Some(Frame::List {
                        ordered,
                        start,
                        items,
                    }) = stack.pop()
                    {
                        emit(
                            &mut stack,
                            Block::List {
                                ordered,
                                start,
                                items,
                            },
                        );
                    }
                }
                TagEnd::Item => {
                    flush_inline(&mut stack, &mut inline);
                    if let Some(Frame::Item(v)) = stack.pop()
                        && let Some(Frame::List { items, .. }) = stack.last_mut()
                    {
                        items.push(v);
                    }
                }
                TagEnd::TableCell => row.push(std::mem::take(&mut inline)),
                TagEnd::TableHead => {
                    in_head = false;
                    if let Some((head, _, _)) = table.as_mut() {
                        *head = std::mem::take(&mut row);
                    }
                }
                TagEnd::TableRow => {
                    if !in_head && let Some((_, rows, _)) = table.as_mut() {
                        rows.push(std::mem::take(&mut row));
                    }
                }
                TagEnd::Table => {
                    if let Some((head, rows, right)) = table.take() {
                        emit(&mut stack, Block::Table { head, rows, right });
                    }
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                    style = styles.pop().unwrap_or_default();
                }
                _ => {}
            },
            Event::Text(t) => {
                if let Some((_, body)) = code.as_mut() {
                    body.push_str(&t);
                } else {
                    inline.push(&t, &style);
                }
            }
            Event::Code(t) => {
                let mut s = style.clone();
                s.code = true;
                inline.push(&t, &s);
            }
            Event::SoftBreak => inline.push("\n", &style),
            Event::HardBreak => inline.push("\n", &style),
            Event::Rule => {
                flush_inline(&mut stack, &mut inline);
                emit(&mut stack, Block::Rule);
            }
            Event::Html(h) | Event::InlineHtml(h) => inline.push(&h, &style),
            _ => {}
        }
    }
    flush_inline(&mut stack, &mut inline);
    while stack.len() > 1 {
        // Defensive: unbalanced input never loses what was parsed.
        match stack.pop() {
            Some(Frame::Quote(v)) => emit(&mut stack, Block::Quote(v)),
            Some(Frame::Item(v)) => {
                if let Some(Frame::List { items, .. }) = stack.last_mut() {
                    items.push(v);
                }
            }
            Some(Frame::List {
                ordered,
                start,
                items,
            }) => emit(
                &mut stack,
                Block::List {
                    ordered,
                    start,
                    items,
                },
            ),
            _ => {}
        }
    }
    match stack.pop() {
        Some(Frame::Doc(v)) => v,
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn para(text: &str) -> Block {
        Block::Paragraph(Inline {
            text: text.into(),
            spans: vec![],
        })
    }

    #[test]
    fn headings_paragraphs_and_inline_styles() {
        let blocks = parse("# Title\n\nsome **bold** and `code` and [link](http://x)\n");
        assert!(matches!(&blocks[0], Block::Heading { level: 1, text } if text.text == "Title"));
        let Block::Paragraph(p) = &blocks[1] else {
            panic!("paragraph")
        };
        assert_eq!(p.text, "some bold and code and link");
        assert!(
            p.spans
                .iter()
                .any(|(r, s)| s.bold && &p.text[r.clone()] == "bold")
        );
        assert!(
            p.spans
                .iter()
                .any(|(r, s)| s.code && &p.text[r.clone()] == "code")
        );
        assert!(
            p.spans
                .iter()
                .any(|(r, s)| s.link.as_deref() == Some("http://x") && &p.text[r.clone()] == "link")
        );
    }

    #[test]
    fn lists_nest_and_numbering_starts_where_told() {
        let blocks = parse("3. a\n4. b\n   - inner\n");
        let Block::List {
            ordered,
            start,
            items,
        } = &blocks[0]
        else {
            panic!("list")
        };
        assert!(*ordered);
        assert_eq!(*start, 3);
        assert_eq!(items.len(), 2);
        assert!(matches!(items[1][1], Block::List { ordered: false, .. }));
    }

    #[test]
    fn an_unclosed_fence_streams_as_code() {
        assert_eq!(
            parse("intro\n\n```rust\nlet a = 1;\nhalf"),
            vec![
                para("intro"),
                Block::Code {
                    lang: "rust".into(),
                    text: "let a = 1;\nhalf".into()
                }
            ]
        );
    }

    #[test]
    fn closed_fence_drops_the_trailing_newline() {
        assert_eq!(
            parse("```\nx\n```\nafter"),
            vec![
                Block::Code {
                    lang: String::new(),
                    text: "x".into()
                },
                para("after")
            ]
        );
    }

    #[test]
    fn quotes_rules_and_tables() {
        let blocks = parse("> quoted\n\n---\n\n| a | b |\n|---|--:|\n| 1 | 2 |\n");
        assert!(matches!(&blocks[0], Block::Quote(v) if v.len() == 1));
        assert_eq!(blocks[1], Block::Rule);
        let Block::Table { head, rows, right } = &blocks[2] else {
            panic!("table")
        };
        assert_eq!(head.len(), 2);
        assert_eq!(rows.len(), 1);
        assert_eq!(right, &vec![false, true]);
    }

    #[test]
    fn empty_and_partial_input_never_panic() {
        assert!(parse("").is_empty());
        for cut in 0.."# a\n\n- x\n  - y\n\n| a |\n|---|\n| 1 |\n> q\n```rs\nz".len() {
            let src = "# a\n\n- x\n  - y\n\n| a |\n|---|\n| 1 |\n> q\n```rs\nz";
            if src.is_char_boundary(cut) {
                let _ = parse(&src[..cut]);
            }
        }
    }
}
