//! Pure logic behind tool cards, activity groups and the per-type renderers
//! (PDA-T-0069, PDA-T-0070): classification, one-line summaries and the data each renderer shows.

use std::time::Duration;

use pando_client::Item;
use pando_client::model::{ItemKind, ToolItem, ToolStatus};
use serde_json::Value;

/// Longest output a card shows before "show more".
pub const OUTPUT_CHARS: usize = 2_000;
/// Longest one-line summary.
const SUMMARY_CHARS: usize = 100;

/// How a tool is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolClass {
    /// Runs a command.
    Execute,
    /// Edits a file.
    Edit,
    /// Writes a file.
    Write,
    /// Reads a file.
    Read,
    /// Searches (grep, glob, ls, tool search).
    Search,
    /// Fetches a URL.
    Fetch,
    /// Reasoning helper.
    Think,
    /// Anything else (generic JSON).
    Other,
}

impl ToolClass {
    /// Every class, in summary order.
    pub const ALL: [ToolClass; 8] = [
        Self::Execute,
        Self::Edit,
        Self::Write,
        Self::Read,
        Self::Search,
        Self::Fetch,
        Self::Think,
        Self::Other,
    ];

    /// Locale key suffix.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Execute => "execute",
            Self::Edit => "edit",
            Self::Write => "write",
            Self::Read => "read",
            Self::Search => "search",
            Self::Fetch => "fetch",
            Self::Think => "think",
            Self::Other => "other",
        }
    }
}

/// Classifies a tool by Pando's `kind`, falling back to the tool name.
#[must_use]
pub fn classify(tool: &ToolItem) -> ToolClass {
    let name = tool.name.to_ascii_lowercase();
    match tool.kind.as_str() {
        "execute" => return ToolClass::Execute,
        "read" => return ToolClass::Read,
        "search" => return ToolClass::Search,
        "fetch" => return ToolClass::Fetch,
        "think" => return ToolClass::Think,
        "edit" => {
            return if name.contains("write") {
                ToolClass::Write
            } else {
                ToolClass::Edit
            };
        }
        _ => {}
    }
    match name.as_str() {
        "bash" | "shell" | "run" => ToolClass::Execute,
        "edit" | "patch" | "multiedit" | "multi_edit" => ToolClass::Edit,
        "write" => ToolClass::Write,
        "view" | "read" | "cat" => ToolClass::Read,
        "grep" | "glob" | "ls" | "find" | "tool_search" | "search" => ToolClass::Search,
        "fetch" | "web_fetch" | "webfetch" | "download" => ToolClass::Fetch,
        _ => ToolClass::Other,
    }
}

/// Parsed JSON input of a call (`Null` while it is still streaming or when it is not JSON).
#[must_use]
pub fn input_json(tool: &ToolItem) -> Value {
    serde_json::from_str(&tool.input).unwrap_or(Value::Null)
}

fn first_str<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| v.get(k).and_then(Value::as_str))
        .filter(|s| !s.is_empty())
}

/// Truncates to `max` characters with an ellipsis.
#[must_use]
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// A one-line summary of what the call is about (command, path, pattern, URL, ...).
#[must_use]
pub fn arg_summary(tool: &ToolItem) -> String {
    let input = input_json(tool);
    let picked = match classify(tool) {
        ToolClass::Execute => first_str(&input, &["command", "cmd", "script"]),
        ToolClass::Edit | ToolClass::Write | ToolClass::Read => {
            first_str(&input, &["file_path", "path", "filePath"])
        }
        ToolClass::Search => first_str(&input, &["pattern", "query", "q", "path"]),
        ToolClass::Fetch => first_str(&input, &["url", "uri"]),
        _ => None,
    };
    let text = picked
        .map(str::to_owned)
        .or_else(|| tool.locations.first().cloned())
        .or_else(|| {
            input.as_object().and_then(|o| {
                o.values()
                    .find_map(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned))
            })
        })
        .unwrap_or_else(|| tool.title.clone());
    truncate_chars(text.lines().next().unwrap_or(""), SUMMARY_CHARS)
}

/// "320 ms", "4.2 s", "1 min 5 s".
#[must_use]
pub fn duration_text(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1_000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", d.as_secs_f64())
    } else {
        format!("{} min {} s", d.as_secs() / 60, d.as_secs() % 60)
    }
}

/// Pretty JSON of `raw`, or `raw` itself when it is not JSON.
#[must_use]
pub fn pretty_json(raw: &str) -> String {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| raw.to_owned())
}

/// Whether the call is still going.
#[must_use]
pub fn is_active(tool: &ToolItem) -> bool {
    matches!(tool.status, ToolStatus::Pending | ToolStatus::Running)
}

/// One element of the transcript layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A single item, by index into `ChatModel::items()`.
    Item(usize),
    /// A run of consecutive tool calls folded together (indices into the items).
    Activity(Vec<usize>),
}

/// Lays the items out as rows: consecutive tool calls become one [`Row::Activity`].
#[must_use]
pub fn layout(items: &[Item]) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if matches!(item.kind, ItemKind::Tool(_)) {
            if let Some(Row::Activity(ix)) = rows.last_mut() {
                ix.push(i);
            } else {
                rows.push(Row::Activity(vec![i]));
            }
        } else {
            rows.push(Row::Item(i));
        }
    }
    rows
}

/// How many calls of each class a group holds, in [`ToolClass::ALL`] order, zero counts dropped.
#[must_use]
pub fn class_counts(tools: &[&ToolItem]) -> Vec<(ToolClass, usize)> {
    ToolClass::ALL
        .iter()
        .filter_map(|class| {
            let n = tools.iter().filter(|t| classify(t) == *class).count();
            (n > 0).then_some((*class, n))
        })
        .collect()
}

/// Aggregate state of a group: `(running, failed)`.
#[must_use]
pub fn group_state(tools: &[&ToolItem]) -> (usize, usize) {
    (
        tools.iter().filter(|t| is_active(t)).count(),
        tools
            .iter()
            .filter(|t| {
                t.status == ToolStatus::Failed || t.outcome.as_ref().is_some_and(|o| o.is_error)
            })
            .count(),
    )
}

// ------------------------------------------------------------------ per-type renderer data

/// Text cut for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clipped {
    /// The visible text.
    pub text: String,
    /// More text exists.
    pub truncated: bool,
    /// Total characters.
    pub total: usize,
}

/// Cuts `s` to `max` characters.
#[must_use]
pub fn clip(s: &str, max: usize) -> Clipped {
    let total = s.chars().count();
    if total <= max {
        return Clipped {
            text: s.to_owned(),
            truncated: false,
            total,
        };
    }
    Clipped {
        text: s.chars().take(max).collect(),
        truncated: true,
        total,
    }
}

/// `execute`: `$ command`, output and exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecView {
    /// The command line.
    pub command: String,
    /// Output so far.
    pub output: Clipped,
    /// Exit code, when Pando reported one.
    pub exit_code: Option<i64>,
}

/// Data for the `execute` renderer.
#[must_use]
pub fn exec_view(tool: &ToolItem) -> ExecView {
    let input = input_json(tool);
    let command = first_str(&input, &["command", "cmd", "script"])
        .map_or_else(|| tool.title.clone(), str::to_owned);
    let outcome = tool.outcome.as_ref();
    ExecView {
        command,
        output: clip(outcome.map_or("", |o| o.content.as_str()), OUTPUT_CHARS),
        exit_code: outcome
            .and_then(|o| o.terminal.as_ref())
            .and_then(|t| t.get("exit_code"))
            .and_then(Value::as_i64),
    }
}

/// `read`: path and the first characters of the content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadView {
    /// File.
    pub path: String,
    /// Content (2,000 characters at most).
    pub content: Clipped,
}

/// Data for the `read` renderer.
#[must_use]
pub fn read_view(tool: &ToolItem) -> ReadView {
    let input = input_json(tool);
    ReadView {
        path: first_str(&input, &["file_path", "path", "filePath"])
            .map(str::to_owned)
            .or_else(|| tool.locations.first().cloned())
            .unwrap_or_else(|| tool.title.clone()),
        content: clip(
            tool.outcome.as_ref().map_or("", |o| o.content.as_str()),
            OUTPUT_CHARS,
        ),
    }
}

/// `search`/`grep`/`glob`: what was looked for and the first result lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchView {
    /// Pattern or query.
    pub query: String,
    /// Result lines in total.
    pub total: usize,
    /// The first lines.
    pub lines: Vec<String>,
}

/// Lines a search card shows.
pub const SEARCH_LINES: usize = 8;

/// Data for the `search` renderer.
#[must_use]
pub fn search_view(tool: &ToolItem) -> SearchView {
    let input = input_json(tool);
    let query = first_str(&input, &["pattern", "query", "q", "path"])
        .map_or_else(|| tool.title.clone(), str::to_owned);
    let content = tool.outcome.as_ref().map_or("", |o| o.content.as_str());
    let all: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    SearchView {
        query,
        total: all.len(),
        lines: all
            .iter()
            .take(SEARCH_LINES)
            .map(|l| truncate_chars(l, 160))
            .collect(),
    }
}

/// `fetch`: URL and a summary of the response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchView {
    /// URL.
    pub url: String,
    /// Response text clipped.
    pub summary: Clipped,
}

/// Data for the `fetch` renderer.
#[must_use]
pub fn fetch_view(tool: &ToolItem) -> FetchView {
    let input = input_json(tool);
    FetchView {
        url: first_str(&input, &["url", "uri"]).map_or_else(|| tool.title.clone(), str::to_owned),
        summary: clip(
            tool.outcome.as_ref().map_or("", |o| o.content.as_str()),
            OUTPUT_CHARS,
        ),
    }
}

/// Generic fallback: pretty input and output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonView {
    /// Pretty-printed input.
    pub input: Clipped,
    /// Output text or pretty raw output.
    pub output: Clipped,
}

/// Data for the generic renderer.
#[must_use]
pub fn json_view(tool: &ToolItem) -> JsonView {
    let out = tool.outcome.as_ref().map_or_else(String::new, |o| {
        if o.content.is_empty() && !o.raw_output.is_null() {
            serde_json::to_string_pretty(&o.raw_output).unwrap_or_default()
        } else {
            pretty_json(&o.content)
        }
    });
    JsonView {
        input: clip(&pretty_json(&tool.input), OUTPUT_CHARS),
        output: clip(&out, OUTPUT_CHARS),
    }
}

/// Images attached to the result (`data:` URLs).
#[must_use]
pub fn images(tool: &ToolItem) -> &[String] {
    tool.outcome.as_ref().map_or(&[], |o| o.images.as_slice())
}

/// Image formats a result may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    /// PNG.
    Png,
    /// JPEG.
    Jpeg,
    /// GIF.
    Gif,
    /// WebP.
    Webp,
}

fn b64_value(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

/// Decodes standard or URL-safe base64 (padding optional, whitespace ignored).
#[must_use]
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0_u32, 0_u32);
    for b in s.bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | u32::from(b64_value(b)?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xFF).ok()?);
        }
    }
    Some(out)
}

/// Decodes a `data:image/...;base64,` URL.
#[must_use]
pub fn decode_data_url(url: &str) -> Option<(ImageKind, Vec<u8>)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if !meta.contains("base64") {
        return None;
    }
    let mime = meta.split(';').next().unwrap_or("");
    let kind = match mime {
        "image/png" => ImageKind::Png,
        "image/jpeg" | "image/jpg" => ImageKind::Jpeg,
        "image/gif" => ImageKind::Gif,
        "image/webp" => ImageKind::Webp,
        _ => return None,
    };
    Some((kind, base64_decode(data)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::testutil::tool;

    #[test]
    fn data_urls_decode_to_bytes() {
        // "hello" in base64
        assert_eq!(base64_decode("aGVsbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64_decode("aGVsbG8").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64_decode("a$b"), None);
        let (k, b) = decode_data_url("data:image/png;base64,aGk=").expect("png");
        assert_eq!((k, b.as_slice()), (ImageKind::Png, &b"hi"[..]));
        assert!(decode_data_url("data:text/plain;base64,aGk=").is_none());
        assert!(decode_data_url("https://x/y.png").is_none());
    }

    use pando_client::chat::{ToolCallEvent, ToolResultEvent};
    use serde_json::json;

    fn call(name: &str, kind: &str, input: Value, content: &str) -> ToolItem {
        tool(
            ToolCallEvent {
                id: format!("{name}-1"),
                name: name.into(),
                kind: kind.into(),
                title: format!("{name} title"),
                input: input.to_string(),
                ..ToolCallEvent::default()
            },
            Some(ToolResultEvent {
                tool_call_id: format!("{name}-1"),
                status: "completed".into(),
                content: content.into(),
                ..ToolResultEvent::default()
            }),
        )
    }

    #[test]
    fn classification_prefers_kind_then_name() {
        assert_eq!(
            classify(&call("bash", "execute", json!({}), "")),
            ToolClass::Execute
        );
        assert_eq!(
            classify(&call("write", "edit", json!({}), "")),
            ToolClass::Write
        );
        assert_eq!(
            classify(&call("edit", "edit", json!({}), "")),
            ToolClass::Edit
        );
        assert_eq!(
            classify(&call("grep", "other", json!({}), "")),
            ToolClass::Search
        );
        assert_eq!(
            classify(&call("mystery", "other", json!({}), "")),
            ToolClass::Other
        );
    }

    #[test]
    fn summaries_pick_the_meaningful_argument() {
        let c = call(
            "bash",
            "execute",
            json!({"command": "cargo test\nsecond"}),
            "",
        );
        assert_eq!(arg_summary(&c), "cargo test");
        let r = call("view", "read", json!({"file_path": "src/a.rs"}), "");
        assert_eq!(arg_summary(&r), "src/a.rs");
        let o = call("odd", "other", json!({"x": 1}), "");
        assert_eq!(arg_summary(&o), "odd title");
        let long = call("bash", "execute", json!({"command": "x".repeat(300)}), "");
        assert_eq!(arg_summary(&long).chars().count(), SUMMARY_CHARS + 1);
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration_text(Duration::from_millis(320)), "320 ms");
        assert_eq!(duration_text(Duration::from_millis(4200)), "4.2 s");
        assert_eq!(duration_text(Duration::from_secs(65)), "1 min 5 s");
    }

    #[test]
    fn consecutive_tools_fold_into_one_activity_row() {
        let mut model = pando_client::ChatModel::default();
        use pando_client::chat::ChatEvent as E;
        model.apply(&E::ContentDelta("hi".into()));
        for id in ["a", "b", "c"] {
            model.apply(&E::ToolCall(ToolCallEvent {
                id: id.into(),
                name: "bash".into(),
                kind: "execute".into(),
                ..ToolCallEvent::default()
            }));
        }
        model.apply(&E::ContentDelta("done".into()));
        // A repeated update of a call must not add a row.
        model.apply(&E::ToolCallUpdate(ToolCallEvent {
            id: "a".into(),
            name: "bash".into(),
            kind: "execute".into(),
            ..ToolCallEvent::default()
        }));
        let rows = layout(model.items());
        assert_eq!(rows.len(), 3, "{rows:?}");
        assert_eq!(rows[1], Row::Activity(vec![1, 2, 3]));
    }

    #[test]
    fn group_summary_counts_by_class() {
        let a = call("bash", "execute", json!({}), "");
        let b = call("bash", "execute", json!({}), "");
        let c = call("view", "read", json!({}), "");
        assert_eq!(
            class_counts(&[&a, &b, &c]),
            vec![(ToolClass::Execute, 2), (ToolClass::Read, 1)]
        );
        assert_eq!(group_state(&[&a, &b, &c]), (0, 0));
    }

    #[test]
    fn exec_view_reads_command_output_and_exit_code() {
        let mut t = call("bash", "execute", json!({"command": "ls"}), "a\nb");
        if let Some(o) = t.outcome.as_mut() {
            o.terminal = Some(json!({"terminal_id": "t", "exit_code": 1}));
        }
        let v = exec_view(&t);
        assert_eq!(v.command, "ls");
        assert_eq!(v.exit_code, Some(1));
        assert_eq!(v.output.text, "a\nb");
    }

    #[test]
    fn long_output_is_clipped_with_the_total_known() {
        let t = call("view", "read", json!({"file_path": "f"}), &"x".repeat(5000));
        let v = read_view(&t);
        assert!(v.content.truncated);
        assert_eq!(v.content.text.chars().count(), OUTPUT_CHARS);
        assert_eq!(v.content.total, 5000);
    }

    #[test]
    fn search_view_lists_the_first_lines() {
        let body = (1..=20)
            .map(|i| format!("hit {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let t = call("grep", "search", json!({"pattern": "foo"}), &body);
        let v = search_view(&t);
        assert_eq!(
            (v.query.as_str(), v.total, v.lines.len()),
            ("foo", 20, SEARCH_LINES)
        );
    }

    #[test]
    fn json_fallback_pretty_prints() {
        let t = call("mystery", "other", json!({"a": 1}), "{\"ok\":true}");
        let v = json_view(&t);
        assert!(v.input.text.contains("\n  \"a\": 1"), "{}", v.input.text);
        assert!(v.output.text.contains("\"ok\": true"));
    }
}
