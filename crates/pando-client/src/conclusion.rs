//! Parsers for the delegation conclusion protocol.
//!
//! Ports `internal/mesnada/conclusion/parse.go` (the `<pando:conclusion>` YAML block: last block
//! wins, tolerant, warn-don't-discard) and reads back what `format.go` injects into the parent
//! conversation (`[delegated-result task=... engine=... project=... status=... confidence=...]`
//! followed by `summary:`, `follow_up:`, `artifacts:`, `memory_refs:` and `warnings:`).
//! The conclusion body only uses a tiny YAML subset, so a purpose-built reader avoids a YAML
//! dependency.

use serde::Deserialize;

const OPEN: &str = "<pando:conclusion>";
const CLOSE: &str = "</pando:conclusion>";

/// A task conclusion (also the JSON shape of `TaskResponse.conclusion`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Conclusion {
    /// `success|partial|failed|blocked`, empty when unknown.
    pub status: String,
    /// Summary.
    pub summary: String,
    /// Produced artifacts (paths or refs).
    pub artifacts: Vec<String>,
    /// Memory/KB references.
    pub memory_refs: Vec<String>,
    /// Suggested follow-up.
    pub follow_up: String,
    /// 0..=1.
    pub confidence: f64,
    /// No block was emitted: derived from output/error.
    pub synthesized: bool,
    /// Capture time (RFC 3339) when it came from the server.
    pub captured_at: Option<String>,
    /// Soft-validation warnings.
    pub warnings: Vec<String>,
}

/// Finds the last `<pando:conclusion>` block of `raw` and parses it. `None` when there is none.
pub fn parse_conclusion(raw: &str) -> Option<Conclusion> {
    let close = raw.rfind(CLOSE)?;
    let open = raw[..close].rfind(OPEN)?;
    let inner = &raw[open + OPEN.len()..close];

    let mut c = Conclusion::default();
    let Some(body) = parse_body(inner) else {
        c.summary = inner.trim().to_owned();
        c.warnings
            .push("conclusion body was not valid YAML; salvaged raw text".to_owned());
        return Some(c);
    };

    let raw_status = body.status.trim();
    c.status = normalize_status(raw_status).to_owned();
    if !raw_status.is_empty() && c.status.is_empty() {
        c.warnings.push(format!(
            "unknown status {raw_status:?}; left for software default"
        ));
    }
    if !(0.0..=1.0).contains(&body.confidence) {
        c.warnings.push(format!(
            "confidence {} out of range [0,1]; clamped",
            body.confidence
        ));
    }
    c.confidence = body.confidence.clamp(0.0, 1.0);
    c.summary = body.summary.trim().to_owned();
    c.follow_up = body.follow_up.trim().to_owned();
    c.artifacts = filter_blanks(body.artifacts, "artifacts", &mut c.warnings);
    c.memory_refs = filter_blanks(body.memory_refs, "memory_refs", &mut c.warnings);

    if c.summary.is_empty()
        && c.status.is_empty()
        && c.artifacts.is_empty()
        && c.memory_refs.is_empty()
        && c.follow_up.is_empty()
    {
        c.summary = inner.trim().to_owned();
        c.warnings
            .push("conclusion body was not a YAML mapping; salvaged raw text".to_owned());
    } else if c.summary.is_empty() {
        c.warnings.push("missing summary".to_owned());
    }
    Some(c)
}

fn normalize_status(s: &str) -> &'static str {
    match s.trim().to_ascii_lowercase().as_str() {
        "success" => "success",
        "partial" => "partial",
        "failed" => "failed",
        "blocked" => "blocked",
        _ => "",
    }
}

fn filter_blanks(items: Vec<String>, field: &str, warns: &mut Vec<String>) -> Vec<String> {
    let total = items.len();
    let kept: Vec<String> = items.into_iter().filter(|s| !s.trim().is_empty()).collect();
    let dropped = total - kept.len();
    if dropped > 0 {
        let noun = if dropped == 1 { "entry" } else { "entries" };
        warns.push(format!("dropped {dropped} blank {field} {noun}"));
    }
    kept
}

#[derive(Default)]
struct Body {
    status: String,
    summary: String,
    artifacts: Vec<String>,
    memory_refs: Vec<String>,
    follow_up: String,
    confidence: f64,
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return v[1..v.len() - 1].to_owned();
        }
    }
    v.to_owned()
}

fn is_quoted(v: &str) -> bool {
    let v = v.trim();
    v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
}

fn is_key(k: &str) -> bool {
    !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Where the lines under the current key go.
enum Cur {
    None,
    Scalar(String),
    Block {
        key: String,
        folded: bool,
        lines: Vec<String>,
    },
    List(String),
}

/// Reads the YAML subset used by conclusions. `None` means "not valid YAML" (what a real YAML
/// parser would reject: unquoted `: ` inside a value, junk lines, bad numbers).
fn parse_body(inner: &str) -> Option<Body> {
    let mut map: Vec<(String, Value)> = Vec::new();
    let mut cur = Cur::None;

    fn flush(cur: &mut Cur, map: &mut Vec<(String, Value)>) {
        match std::mem::replace(cur, Cur::None) {
            Cur::Block { key, folded, lines } => {
                let text = if folded {
                    lines.join(" ")
                } else {
                    lines.join("\n")
                };
                map.push((key, Value::Scalar(text.trim_end().to_owned())));
            }
            Cur::None | Cur::Scalar(_) | Cur::List(_) => {}
        }
    }

    for line in inner.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            if let Cur::Block { lines, .. } = &mut cur {
                lines.push(String::new());
            }
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let t = line.trim();
        if indent == 0 && !(t.starts_with("- ") || t == "-") {
            flush(&mut cur, &mut map);
            let (k, v) = match t.split_once(':') {
                Some((k, v)) if v.is_empty() || v.starts_with(' ') => (k, v.trim()),
                _ => return None,
            };
            if !is_key(k) {
                return None;
            }
            match v {
                "" => {
                    map.push((k.to_owned(), Value::List(Vec::new())));
                    cur = Cur::List(k.to_owned());
                }
                "|" | "|-" | ">" | ">-" => {
                    cur = Cur::Block {
                        key: k.to_owned(),
                        folded: v.starts_with('>'),
                        lines: Vec::new(),
                    };
                }
                _ if v.starts_with('[') => {
                    let inner = v.strip_prefix('[')?.strip_suffix(']')?;
                    let items = inner
                        .split(',')
                        .map(unquote)
                        .filter(|s| !s.is_empty())
                        .collect();
                    map.push((k.to_owned(), Value::List(items)));
                }
                _ => {
                    if !is_quoted(v) && (v.contains(": ") || v.ends_with(':')) {
                        return None;
                    }
                    map.push((k.to_owned(), Value::Scalar(unquote(v))));
                    cur = Cur::Scalar(k.to_owned());
                }
            }
            continue;
        }
        // indented line, or a list item at indent 0
        match &mut cur {
            Cur::List(key) => {
                let item = t
                    .strip_prefix("- ")
                    .or(if t == "-" { Some("") } else { None })?;
                if let Some((_, Value::List(items))) = map.iter_mut().rev().find(|(k, _)| k == key)
                {
                    items.push(unquote(item));
                }
            }
            Cur::Block { lines, .. } => lines.push(t.to_owned()),
            Cur::Scalar(key) => {
                // plain multi-line scalar continuation
                let key = key.clone();
                if let Some((_, Value::Scalar(s))) = map.iter_mut().rev().find(|(k, _)| *k == key) {
                    s.push(' ');
                    s.push_str(t);
                }
            }
            Cur::None => return None,
        }
    }
    flush(&mut cur, &mut map);

    let mut b = Body::default();
    for (k, v) in map {
        match (k.as_str(), v) {
            ("status", Value::Scalar(s)) => b.status = s,
            ("summary", Value::Scalar(s)) => b.summary = s,
            ("follow_up", Value::Scalar(s)) => b.follow_up = s,
            ("confidence", Value::Scalar(s)) => {
                b.confidence = if s.trim().is_empty() {
                    0.0
                } else {
                    s.trim().parse().ok()?
                }
            }
            ("artifacts", Value::List(l)) => b.artifacts = l,
            ("memory_refs", Value::List(l)) => b.memory_refs = l,
            // A scalar where a list is expected, or unknown keys: lenient.
            ("artifacts", Value::Scalar(s)) => b.artifacts = vec![s],
            ("memory_refs", Value::Scalar(s)) => b.memory_refs = vec![s],
            _ => {}
        }
    }
    Some(b)
}

enum Value {
    Scalar(String),
    List(Vec<String>),
}

// ------------------------------------------------------------------ [delegated-result]

/// A `[delegated-result ...]` block found in a session history message.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DelegatedResult {
    /// Task id.
    pub task: String,
    /// Engine (`claude`, `pando`, ...).
    pub engine: String,
    /// Project label.
    pub project: String,
    /// Conclusion status, or the task status when no conclusion was captured.
    pub status: String,
    /// Confidence when the header carried one.
    pub confidence: Option<f64>,
    /// Summary (may span lines).
    pub summary: String,
    /// Follow-up.
    pub follow_up: String,
    /// Artifacts.
    pub artifacts: Vec<String>,
    /// Memory refs.
    pub memory_refs: Vec<String>,
    /// Warnings.
    pub warnings: Vec<String>,
    /// The task finished without a captured conclusion.
    pub no_conclusion: bool,
}

const HEADER: &str = "[delegated-result";
const KEYS: [&str; 5] = ["task", "engine", "project", "status", "confidence"];

/// Parses every `[delegated-result ...]` block of `text`, in order.
pub fn parse_delegated_results(text: &str) -> Vec<DelegatedResult> {
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(start) = line.find(HEADER) else {
            continue;
        };
        let rest = &line[start + HEADER.len()..];
        let Some(end) = rest.rfind(']') else {
            continue;
        };
        let mut r = parse_header(&rest[..end]);
        r.no_conclusion = rest[end + 1..].contains("no conclusion captured");
        // Body: field lines until the next header or a non-field line after the fields.
        let mut field: Option<&str> = None;
        while let Some(next) = lines.peek() {
            if next.contains(HEADER) {
                break;
            }
            let (name, value) = split_field(next);
            if let Some(name) = name {
                field = Some(name);
                set_field(&mut r, name, value);
            } else if let Some(f) = field {
                let t = next.trim();
                if let Some(item) = t.strip_prefix("- ") {
                    push_item(&mut r, f, item);
                } else if !t.is_empty() && f == "summary" {
                    r.summary.push('\n');
                    r.summary.push_str(t);
                } else if !t.is_empty() && f == "follow_up" {
                    r.follow_up.push('\n');
                    r.follow_up.push_str(t);
                } else if t.is_empty() {
                    // blank line ends nothing by itself
                } else {
                    break;
                }
            } else {
                break;
            }
            lines.next();
        }
        out.push(r);
    }
    out
}

/// Parses the first `[delegated-result ...]` block of `text`.
pub fn parse_delegated_result(text: &str) -> Option<DelegatedResult> {
    parse_delegated_results(text).into_iter().next()
}

fn parse_header(h: &str) -> DelegatedResult {
    // Locate " key=" markers; a value runs to the next marker (project names may contain spaces).
    let mut marks: Vec<(usize, &str)> = Vec::new();
    for key in KEYS {
        let pat = format!(" {key}=");
        if let Some(i) = h.find(&pat) {
            marks.push((i, key));
        }
    }
    marks.sort();
    let mut r = DelegatedResult::default();
    for (n, (i, key)) in marks.iter().enumerate() {
        let from = i + key.len() + 2;
        let to = marks.get(n + 1).map_or(h.len(), |m| m.0);
        let v = h[from..to].trim().to_owned();
        match *key {
            "task" => r.task = v,
            "engine" => r.engine = v,
            "project" => r.project = v,
            "status" => r.status = v,
            _ => r.confidence = v.parse().ok(),
        }
    }
    r
}

fn split_field(line: &str) -> (Option<&'static str>, &str) {
    for f in [
        "summary",
        "follow_up",
        "artifacts",
        "memory_refs",
        "warnings",
    ] {
        if let Some(v) = line.strip_prefix(f).and_then(|r| r.strip_prefix(':')) {
            return (Some(f), v.trim());
        }
    }
    (None, line)
}

fn split_list(v: &str) -> Vec<String> {
    v.split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

fn set_field(r: &mut DelegatedResult, name: &str, v: &str) {
    match name {
        "summary" => r.summary = v.to_owned(),
        "follow_up" => r.follow_up = v.to_owned(),
        "artifacts" => r.artifacts = split_list(v),
        "memory_refs" => r.memory_refs = split_list(v),
        _ => r.warnings = split_list(v),
    }
}

fn push_item(r: &mut DelegatedResult, field: &str, item: &str) {
    let item = item.trim().to_owned();
    match field {
        "artifacts" => r.artifacts.push(item),
        "memory_refs" => r.memory_refs.push(item),
        "warnings" => r.warnings.push(item),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(w: &[String], s: &str) -> bool {
        w.iter().any(|x| x.contains(s))
    }

    fn wrap(body: &str) -> String {
        format!("{OPEN}\n{body}\n{CLOSE}")
    }

    #[test]
    fn happy_block() {
        let raw = "Some preamble.\n<pando:conclusion>\nstatus: success\nsummary: Did the thing well.\nartifacts:\n  - a/b.go\n  - c/d.go\nmemory_refs:\n  - kb-1\nfollow_up: nothing\nconfidence: 0.8\n</pando:conclusion>\ntrailing";
        let c = parse_conclusion(raw).unwrap_or_else(|| panic!("no block"));
        assert_eq!(c.status, "success");
        assert_eq!(c.summary, "Did the thing well.");
        assert_eq!(c.artifacts, vec!["a/b.go", "c/d.go"]);
        assert_eq!(c.memory_refs, vec!["kb-1"]);
        assert_eq!(c.follow_up, "nothing");
        assert_eq!(c.confidence, 0.8);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    }

    #[test]
    fn missing_block() {
        assert!(parse_conclusion("no sentinel here at all").is_none());
        assert!(parse_conclusion("<pando:conclusion>\nsummary: x").is_none());
    }

    #[test]
    fn partial_fields() {
        let c = parse_conclusion(&wrap("summary: only a summary")).unwrap_or_default();
        assert_eq!(c.summary, "only a summary");
        assert_eq!(c.status, "");
        assert_eq!(c.confidence, 0.0);
    }

    #[test]
    fn last_block_wins() {
        let raw = "<pando:conclusion>\nstatus: partial\nsummary: first\n</pando:conclusion>\nmore\n<pando:conclusion>\nstatus: success\nsummary: final\n</pando:conclusion>";
        let c = parse_conclusion(raw).unwrap_or_default();
        assert_eq!(
            (c.summary.as_str(), c.status.as_str()),
            ("final", "success")
        );
    }

    #[test]
    fn malformed_yaml_is_salvaged_with_warning() {
        let c = parse_conclusion(&wrap("this is: not: valid: yaml: ][\n  - broken"))
            .unwrap_or_default();
        assert!(!c.summary.is_empty());
        assert!(has(&c.warnings, "not valid YAML"));
        assert!(has(&c.warnings, "salvaged raw text"));
    }

    #[test]
    fn non_mapping_is_salvaged() {
        let c = parse_conclusion(&wrap("unknown_field: some value\nanother_field: 42"))
            .unwrap_or_default();
        assert!(!c.summary.is_empty());
        assert!(has(&c.warnings, "not a YAML mapping"));
    }

    #[test]
    fn status_normalization() {
        for (input, want) in [
            ("  SUCCESS ", "success"),
            ("Partial", "partial"),
            ("FAILED", "failed"),
            ("blocked", "blocked"),
            ("weird", ""),
            ("", ""),
        ] {
            let c = parse_conclusion(&wrap(&format!("status: \"{input}\"\nsummary: s")))
                .unwrap_or_default();
            assert_eq!(c.status, want, "input {input:?}");
        }
    }

    #[test]
    fn unknown_status_warns_and_keeps_data() {
        let c = parse_conclusion(&wrap("status: done\nsummary: finished")).unwrap_or_default();
        assert_eq!(c.summary, "finished");
        assert_eq!(c.status, "");
        assert!(has(&c.warnings, "unknown status"));
        assert!(has(&c.warnings, "\"done\""));
    }

    #[test]
    fn confidence_is_clamped_with_warning() {
        for (v, want) in [("5.0", 1.0), ("-3", 0.0), ("1.5", 1.0), ("-0.1", 0.0)] {
            let c = parse_conclusion(&wrap(&format!("summary: s\nconfidence: {v}")))
                .unwrap_or_default();
            assert_eq!(c.confidence, want);
            assert!(has(&c.warnings, "out of range"));
        }
    }

    #[test]
    fn missing_summary_warns() {
        let c = parse_conclusion(&wrap("status: success")).unwrap_or_default();
        assert!(has(&c.warnings, "missing summary"));
    }

    #[test]
    fn blank_entries_dropped() {
        let c = parse_conclusion(&wrap("status: success\nsummary: done\nartifacts:\n  - valid/file.go\n  - \"   \"\n  - another/file.go\n  - \"\"\nmemory_refs:\n  - \"\"")).unwrap_or_default();
        assert_eq!(c.artifacts, vec!["valid/file.go", "another/file.go"]);
        assert!(has(&c.warnings, "dropped 2 blank artifacts entries"));
        assert!(has(&c.warnings, "dropped 1 blank memory_refs entry"));
    }

    #[test]
    fn warnings_accumulate() {
        let c =
            parse_conclusion(&wrap("status: done\nsummary: s\nconfidence: 99")).unwrap_or_default();
        assert!(has(&c.warnings, "unknown status") && has(&c.warnings, "out of range"));
        assert_eq!((c.summary.as_str(), c.confidence), ("s", 1.0));
    }

    #[test]
    fn block_scalars_and_inline_lists() {
        let c = parse_conclusion(&wrap("status: success\nsummary: |\n  line one\n  line two\nartifacts: [a.rs, \"b c.rs\"]\nfollow_up: >\n  wrap\n  me")).unwrap_or_default();
        assert_eq!(c.summary, "line one\nline two");
        assert_eq!(c.follow_up, "wrap me");
        assert_eq!(c.artifacts, vec!["a.rs", "b c.rs"]);
    }

    #[test]
    fn delegated_result_full() {
        let text = "[delegated-result task=t-1 engine=claude project=my app status=success confidence=0.90]\nsummary: Fixed the bug\nand verified it\nfollow_up: ship it\nartifacts: a.go, b.go\nmemory_refs: kb-1\nwarnings: missing summary";
        let r = parse_delegated_result(text).unwrap_or_default();
        assert_eq!(r.task, "t-1");
        assert_eq!(r.engine, "claude");
        assert_eq!(r.project, "my app");
        assert_eq!(r.status, "success");
        assert_eq!(r.confidence, Some(0.9));
        assert_eq!(r.summary, "Fixed the bug\nand verified it");
        assert_eq!(r.follow_up, "ship it");
        assert_eq!(r.artifacts, vec!["a.go", "b.go"]);
        assert_eq!(r.memory_refs, vec!["kb-1"]);
        assert_eq!(r.warnings, vec!["missing summary"]);
        assert!(!r.no_conclusion);
    }

    #[test]
    fn delegated_result_resolver_lists_and_no_conclusion() {
        let text = "intro\n[delegated-result task=a engine=pando project=p status=failed]\nsummary: boom\nartifacts:\n  - x.go (1 KB)\n  - y.go\n[delegated-result task=b engine=pando project=unknown status=completed] (no conclusion captured)";
        let all = parse_delegated_results(text);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].artifacts, vec!["x.go (1 KB)", "y.go"]);
        assert_eq!(all[0].confidence, None);
        assert!(all[1].no_conclusion);
        assert_eq!(all[1].status, "completed");
        assert!(parse_delegated_result("nothing here").is_none());
    }
}
