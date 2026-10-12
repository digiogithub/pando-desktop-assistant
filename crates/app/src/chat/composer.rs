//! Pure logic of the composer (PDA-T-0074, PDA-T-0075, PDA-T-0076): character limit, slash
//! palette, prompt history, `@` mentions, dictation insertion and the context meter.

use std::ops::Range;

use pando_client::api::CommandInfo;
use serde_json::Value;

/// Longest prompt, in characters.
pub const LIMIT: usize = 8_000;
/// Remaining characters at which the counter appears.
pub const COUNTER_FROM: usize = 7_000;

/// State of the character counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counter {
    /// Hidden: far from the limit.
    Hidden,
    /// Shown with the current count.
    Shown(usize),
    /// Over the limit: sending is disabled.
    Over(usize),
}

/// Counter state for `text`.
#[must_use]
pub fn counter(text: &str) -> Counter {
    let n = text.chars().count();
    if n > LIMIT {
        Counter::Over(n)
    } else if n >= COUNTER_FROM {
        Counter::Shown(n)
    } else {
        Counter::Hidden
    }
}

/// Whether `text` can be sent.
#[must_use]
pub fn can_send(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= LIMIT
}

// ------------------------------------------------------------------ slash palette

/// The command being typed: `Some("he")` for `/he`, `None` once there is whitespace (arguments
/// follow) or the text is not a command.
#[must_use]
pub fn slash_query(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('/')?;
    if rest.chars().any(char::is_whitespace) {
        return None;
    }
    Some(rest)
}

/// Commands matching `query`: names starting with it first, then names or descriptions
/// containing it. Case-insensitive; an empty query lists everything.
#[must_use]
pub fn filter_commands<'a>(commands: &'a [CommandInfo], query: &str) -> Vec<&'a CommandInfo> {
    let q = query.to_lowercase();
    let mut starts: Vec<&CommandInfo> = Vec::new();
    let mut contains: Vec<&CommandInfo> = Vec::new();
    for c in commands {
        let name = c.name.to_lowercase();
        if name.starts_with(&q) {
            starts.push(c);
        } else if name.contains(&q) || c.description.to_lowercase().contains(&q) {
            contains.push(c);
        }
    }
    starts.extend(contains);
    starts
}

/// The text a chosen command leaves in the composer (`/name ` when it takes arguments, else
/// `/name`). Commands are sent as prompt text.
#[must_use]
pub fn command_text(c: &CommandInfo) -> String {
    if c.accepts_args {
        format!("/{} ", c.name)
    } else {
        format!("/{}", c.name)
    }
}

// ------------------------------------------------------------------ prompt history

/// ↑/↓ history of sent prompts, newest last.
#[derive(Debug, Clone, Default)]
pub struct PromptHistory {
    entries: Vec<String>,
    /// Index being shown, `None` when editing a fresh draft.
    cursor: Option<usize>,
    draft: String,
}

impl PromptHistory {
    /// Records a sent prompt (consecutive duplicates collapse).
    pub fn push(&mut self, text: &str) {
        let text = text.trim();
        if !text.is_empty() && self.entries.last().map(String::as_str) != Some(text) {
            self.entries.push(text.to_owned());
        }
        self.cursor = None;
        self.draft.clear();
    }

    /// ↑: the previous prompt. `current` is the draft, kept to come back to with ↓.
    pub fn older(&mut self, current: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        let next = match self.cursor {
            None => {
                self.draft = current.to_owned();
                self.entries.len() - 1
            }
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.cursor = Some(next);
        self.entries.get(next).map(String::as_str)
    }

    /// ↓: the next prompt, then the draft.
    pub fn newer(&mut self) -> Option<&str> {
        let i = self.cursor?;
        if i + 1 < self.entries.len() {
            self.cursor = Some(i + 1);
            self.entries.get(i + 1).map(String::as_str)
        } else {
            self.cursor = None;
            Some(self.draft.as_str())
        }
    }

    /// The user typed: leave history navigation.
    pub fn reset(&mut self) {
        self.cursor = None;
    }

    /// Browsing history right now.
    #[must_use]
    pub fn is_browsing(&self) -> bool {
        self.cursor.is_some()
    }
}

// ------------------------------------------------------------------ @ mentions

/// An `@query` token under the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    /// Byte offset of the `@`.
    pub start: usize,
    /// Text after the `@`, up to the cursor.
    pub query: String,
}

/// Finds the mention being typed at byte offset `cursor`: an `@` at the start or after
/// whitespace, with no whitespace between it and the cursor.
#[must_use]
pub fn mention_at(text: &str, cursor: usize) -> Option<Mention> {
    let cursor = cursor.min(text.len());
    if !text.is_char_boundary(cursor) {
        return None;
    }
    let before = &text[..cursor];
    let at = before.rfind('@')?;
    let after = &before[at + 1..];
    if after.chars().any(char::is_whitespace) {
        return None;
    }
    let preceded_ok = before[..at]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    preceded_ok.then(|| Mention {
        start: at,
        query: after.to_owned(),
    })
}

/// Replaces the mention by `path` followed by a space. Returns the new text and cursor.
#[must_use]
pub fn apply_mention(text: &str, mention: &Mention, cursor: usize, path: &str) -> (String, usize) {
    // Whatever the cursor says, the whole `@query` token is replaced.
    let end = cursor
        .max(mention.start + 1 + mention.query.len())
        .min(text.len());
    let mut out = String::with_capacity(text.len() + path.len());
    out.push_str(&text[..mention.start]);
    out.push_str(path);
    out.push(' ');
    let pos = out.len();
    out.push_str(&text[end..]);
    (out, pos)
}

/// Paths of a `GET /files/search` answer (`{query, results:[{name, path, isDir, size}]}`):
/// files only, at most `max`.
#[must_use]
pub fn files_from_search(v: &Value, max: usize) -> Vec<String> {
    v.get("results")
        .and_then(Value::as_array)
        .map(|rs| {
            rs.iter()
                .filter(|r| !r.get("isDir").and_then(Value::as_bool).unwrap_or(false))
                .filter_map(|r| r.get("path").and_then(Value::as_str))
                .take(max)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

// ------------------------------------------------------------------ dictation

/// Result of inserting dictated text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dictated {
    /// The whole new text.
    pub text: String,
    /// Where the cursor goes.
    pub cursor: usize,
    /// The inserted range, for the visual "dictado" mark.
    pub mark: Range<usize>,
}

/// Inserts `dictation` at byte offset `cursor`, separated from neighbours by single spaces.
/// Never sends anything: the user reviews and presses send.
#[must_use]
pub fn insert_dictation(text: &str, cursor: usize, dictation: &str) -> Dictated {
    let dictation = dictation.trim();
    let mut at = cursor.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let (head, tail) = text.split_at(at);
    let mut out = String::from(head);
    if !head.is_empty() && !head.ends_with(char::is_whitespace) && !dictation.is_empty() {
        out.push(' ');
    }
    let start = out.len();
    out.push_str(dictation);
    let end = out.len();
    let mut cursor = end;
    if !tail.is_empty() && !tail.starts_with(char::is_whitespace) && !dictation.is_empty() {
        out.push(' ');
        cursor = out.len();
    }
    out.push_str(tail);
    Dictated {
        text: out,
        cursor,
        mark: start..end,
    }
}

/// Editor position (line, UTF-16 column) of byte offset `byte` in `text`.
#[must_use]
pub fn position(text: &str, byte: usize) -> crate::ui::input::Position {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    let before = &text[..byte];
    let line = before.matches('\n').count();
    let col: usize = before
        .rsplit('\n')
        .next()
        .map_or(0, |l| l.encode_utf16().count());
    crate::ui::input::Position::new(
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(col).unwrap_or(u32::MAX),
    )
}

// ------------------------------------------------------------------ context meter

/// Colour band of the context meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterLevel {
    /// Under 70 %.
    Ok,
    /// 70 % or more.
    Warning,
    /// 90 % or more.
    Danger,
}

/// Fraction of the context window in use and its level; `None` when the window is unknown.
#[must_use]
pub fn context_meter(used_tokens: i64, window: i64) -> Option<(f32, MeterLevel)> {
    if window <= 0 {
        return None;
    }
    let ratio = (used_tokens.max(0) as f64 / window as f64).clamp(0.0, 1.0) as f32;
    let level = if ratio >= 0.9 {
        MeterLevel::Danger
    } else if ratio >= 0.7 {
        MeterLevel::Warning
    } else {
        MeterLevel::Ok
    };
    Some((ratio, level))
}

/// Compact token count: `950`, `12.3k`, `1.2M`.
#[must_use]
pub fn compact_tokens(n: i64) -> String {
    let n = n.max(0);
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cmd(name: &str, desc: &str, args: bool) -> CommandInfo {
        CommandInfo {
            name: name.into(),
            description: desc.into(),
            accepts_args: args,
        }
    }

    #[test]
    fn counter_appears_near_the_limit_and_blocks_over_it() {
        assert_eq!(counter("short"), Counter::Hidden);
        assert_eq!(counter(&"é".repeat(7_000)), Counter::Shown(7_000));
        assert_eq!(counter(&"x".repeat(8_001)), Counter::Over(8_001));
        assert!(can_send(&"x".repeat(8_000)));
        assert!(!can_send(&"x".repeat(8_001)));
        assert!(!can_send("  \n "));
    }

    #[test]
    fn slash_query_stops_at_whitespace() {
        assert_eq!(slash_query("/he"), Some("he"));
        assert_eq!(slash_query("/"), Some(""));
        assert_eq!(slash_query("/goal build it"), None);
        assert_eq!(slash_query("hello /x"), None);
    }

    #[test]
    fn commands_rank_prefix_before_contains() {
        let cs = [
            cmd("compact", "Compact the context", false),
            cmd("goal", "Set a goal", true),
            cmd("help", "Show help", false),
            cmd("hello-goal", "x", false),
        ];
        let names = |q: &str| {
            filter_commands(&cs, q)
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(names("go"), ["goal", "hello-goal"]);
        assert_eq!(names("HEL"), ["help", "hello-goal"]);
        assert_eq!(names("context"), ["compact"], "descriptions match too");
        assert_eq!(names("").len(), 4);
        assert_eq!(command_text(&cs[1]), "/goal ");
        assert_eq!(command_text(&cs[0]), "/compact");
    }

    #[test]
    fn history_walks_back_and_returns_to_the_draft() {
        let mut h = PromptHistory::default();
        assert_eq!(h.older("x"), None);
        h.push("one");
        h.push("two");
        h.push("two");
        assert_eq!(h.older("draft"), Some("two"));
        assert_eq!(h.older("ignored"), Some("one"));
        assert_eq!(h.older("ignored"), Some("one"));
        assert_eq!(h.newer(), Some("two"));
        assert_eq!(h.newer(), Some("draft"));
        assert!(!h.is_browsing());
        assert_eq!(h.newer(), None);
    }

    #[test]
    fn mention_is_found_only_in_a_word_start() {
        let m = mention_at("look at @src/ma", 15).expect("mention");
        assert_eq!((m.start, m.query.as_str()), (8, "src/ma"));
        assert_eq!(mention_at("mail me@home", 12), None, "not after text");
        assert_eq!(mention_at("@a b", 4), None, "space ended it");
        assert_eq!(mention_at("@", 1).map(|m| m.query), Some(String::new()));
        assert_eq!(mention_at("é@x", 1), None, "inside a char");
    }

    #[test]
    fn applying_a_mention_inserts_the_relative_path() {
        let text = "see @sr please";
        let m = mention_at(text, 7).expect("mention");
        let (out, cursor) = apply_mention(text, &m, 7, "src/main.rs");
        assert_eq!(out, "see src/main.rs  please");
        assert_eq!(&out[..cursor], "see src/main.rs ");
        // A stale cursor (0) still replaces the token instead of duplicating text.
        let (out, _) = apply_mention(text, &m, 0, "a.rs");
        assert_eq!(out, "see a.rs  please");
    }

    #[test]
    fn search_results_keep_files_only() {
        let v = json!({"query": "a", "results": [
            {"name": "a.rs", "path": "src/a.rs", "isDir": false, "size": 1},
            {"name": "adir", "path": "adir", "isDir": true},
            {"name": "b.rs", "path": "b.rs"},
        ]});
        assert_eq!(files_from_search(&v, 10), ["src/a.rs", "b.rs"]);
        assert_eq!(files_from_search(&v, 1), ["src/a.rs"]);
        assert!(files_from_search(&json!({}), 5).is_empty());
    }

    #[test]
    fn dictation_is_spaced_and_marked() {
        let d = insert_dictation("fix the", 7, "login bug");
        assert_eq!(d.text, "fix the login bug");
        assert_eq!(&d.text[d.mark.clone()], "login bug");
        let d = insert_dictation("ab cd", 3, " hello ");
        assert_eq!(d.text, "ab hello cd");
        assert_eq!(&d.text[d.mark.clone()], "hello");
        assert_eq!(&d.text[..d.cursor], "ab hello ");
        let d = insert_dictation("", 0, "hola");
        assert_eq!((d.text.as_str(), d.cursor), ("hola", 4));
        let d = insert_dictation("x", 1, "  ");
        assert_eq!(d.text, "x");
    }

    #[test]
    fn meter_bands_follow_70_and_90_percent() {
        assert_eq!(context_meter(10, 0), None);
        assert_eq!(context_meter(69, 100).map(|m| m.1), Some(MeterLevel::Ok));
        assert_eq!(
            context_meter(70, 100).map(|m| m.1),
            Some(MeterLevel::Warning)
        );
        assert_eq!(
            context_meter(90, 100).map(|m| m.1),
            Some(MeterLevel::Danger)
        );
        assert_eq!(context_meter(500, 100).map(|m| m.0), Some(1.0));
    }

    #[test]
    fn tokens_are_compact() {
        assert_eq!(compact_tokens(950), "950");
        assert_eq!(compact_tokens(12_345), "12.3k");
        assert_eq!(compact_tokens(1_200_000), "1.2M");
    }
}
