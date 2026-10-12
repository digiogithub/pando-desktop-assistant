//! Diffs of edit/write tool results and the changed-files bar (PDA-T-0070, PDA-T-0071).
//!
//! Pando reports an edit as `diff: {file_path, old_string, new_string}` and a write as
//! `diff: {file_path, new_content}`. The counters come from the diff lines, so they match what
//! the WebUI's `FileChangesBar` computes from the same events.

use pando_client::model::{ItemKind, ToolItem};
use serde_json::Value;
use similar::{ChangeTag, TextDiff};

/// What a diff line does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Unchanged context.
    Same,
    /// Removed.
    Removed,
    /// Added.
    Added,
}

/// One line of a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Change.
    pub kind: LineKind,
    /// Text without the line terminator.
    pub text: String,
}

/// Line diff between two texts.
#[must_use]
pub fn diff_lines(before: &str, after: &str) -> Vec<DiffLine> {
    // A missing final newline must not make the last line look changed.
    let norm = |s: &str| {
        if s.is_empty() || s.ends_with('\n') {
            s.to_owned()
        } else {
            format!("{s}\n")
        }
    };
    let (before, after) = (norm(before), norm(after));
    TextDiff::from_lines(&before, &after)
        .iter_all_changes()
        .map(|c| DiffLine {
            kind: match c.tag() {
                ChangeTag::Equal => LineKind::Same,
                ChangeTag::Delete => LineKind::Removed,
                ChangeTag::Insert => LineKind::Added,
            },
            text: c.value().trim_end_matches(['\n', '\r']).to_owned(),
        })
        .collect()
}

/// The diff of one file as one tool call produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// File path as Pando reported it.
    pub path: String,
    /// Lines (context included).
    pub lines: Vec<DiffLine>,
    /// A newly written file (no previous content known).
    pub is_write: bool,
}

impl FileDiff {
    /// Added line count.
    #[must_use]
    pub fn added(&self) -> usize {
        self.count(LineKind::Added)
    }

    /// Removed line count.
    #[must_use]
    pub fn removed(&self) -> usize {
        self.count(LineKind::Removed)
    }

    fn count(&self, kind: LineKind) -> usize {
        self.lines.iter().filter(|l| l.kind == kind).count()
    }
}

/// Reads a `diff` JSON object.
#[must_use]
pub fn parse_diff(value: &Value) -> Option<FileDiff> {
    let path = value
        .get("file_path")
        .or_else(|| value.get("path"))
        .and_then(Value::as_str)?
        .to_owned();
    let text = |k: &str| value.get(k).and_then(Value::as_str);
    if let Some(new) = text("new_content") {
        return Some(FileDiff {
            path,
            lines: diff_lines("", new),
            is_write: true,
        });
    }
    let (old, new) = (text("old_string"), text("new_string"));
    if old.is_none() && new.is_none() {
        return None;
    }
    Some(FileDiff {
        path,
        lines: diff_lines(old.unwrap_or(""), new.unwrap_or("")),
        is_write: false,
    })
}

/// The diff of a tool call, from its result or (while it still runs) its input.
#[must_use]
pub fn tool_diff(tool: &ToolItem) -> Option<FileDiff> {
    if let Some(d) = tool.outcome.as_ref().and_then(|o| o.diff.as_ref())
        && let Some(diff) = parse_diff(d)
    {
        return Some(diff);
    }
    None
}

/// One file in the changed-files bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    /// Path.
    pub path: String,
    /// Added lines over all edits.
    pub added: usize,
    /// Removed lines over all edits.
    pub removed: usize,
    /// Number of edits that touched it.
    pub edits: usize,
    /// Written from scratch at least once.
    pub written: bool,
    /// Every diff in order (the viewer shows them).
    pub diffs: Vec<FileDiff>,
}

/// Aggregates the edits of a transcript, in order of first change.
#[must_use]
pub fn changed_files<'a>(tools: impl IntoIterator<Item = &'a ToolItem>) -> Vec<ChangedFile> {
    let mut out: Vec<ChangedFile> = Vec::new();
    for tool in tools {
        let Some(diff) = tool_diff(tool) else {
            continue;
        };
        if tool.outcome.as_ref().is_some_and(|o| o.is_error) {
            continue; // a failed edit changed nothing
        }
        let (added, removed) = (diff.added(), diff.removed());
        match out.iter_mut().find(|f| f.path == diff.path) {
            Some(f) => {
                f.added += added;
                f.removed += removed;
                f.edits += 1;
                f.written |= diff.is_write;
                f.diffs.push(diff);
            }
            None => out.push(ChangedFile {
                path: diff.path.clone(),
                added,
                removed,
                edits: 1,
                written: diff.is_write,
                diffs: vec![diff],
            }),
        }
    }
    out
}

/// [`changed_files`] over the items of a model.
#[must_use]
pub fn changed_files_of(items: &[pando_client::Item]) -> Vec<ChangedFile> {
    changed_files(items.iter().filter_map(|i| match &i.kind {
        ItemKind::Tool(t) => Some(t),
        _ => None,
    }))
}

/// Totals `(files, added, removed)` for the bar's headline.
#[must_use]
pub fn totals(files: &[ChangedFile]) -> (usize, usize, usize) {
    (
        files.len(),
        files.iter().map(|f| f.added).sum(),
        files.iter().map(|f| f.removed).sum(),
    )
}

/// Side-by-side rows of a unified diff: `(left, right)` per row, pairing a run of removals with
/// the run of additions that follows it.
#[must_use]
pub fn side_by_side(lines: &[DiffLine]) -> Vec<(Option<DiffLine>, Option<DiffLine>)> {
    let mut rows = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        match lines[i].kind {
            LineKind::Same => {
                rows.push((Some(lines[i].clone()), Some(lines[i].clone())));
                i += 1;
            }
            _ => {
                let mut removed = Vec::new();
                let mut added = Vec::new();
                while i < lines.len() && lines[i].kind != LineKind::Same {
                    if lines[i].kind == LineKind::Removed {
                        removed.push(lines[i].clone());
                    } else {
                        added.push(lines[i].clone());
                    }
                    i += 1;
                }
                for k in 0..removed.len().max(added.len()) {
                    rows.push((removed.get(k).cloned(), added.get(k).cloned()));
                }
            }
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use pando_client::chat::{ToolCallEvent, ToolResultEvent};
    use serde_json::json;

    fn edit(path: &str, old: &str, new: &str) -> ToolItem {
        tool_with(json!({"file_path": path, "old_string": old, "new_string": new}))
    }

    fn tool_with(diff: Value) -> ToolItem {
        crate::chat::testutil::tool(
            ToolCallEvent {
                id: "c".into(),
                name: "edit".into(),
                kind: "edit".into(),
                ..ToolCallEvent::default()
            },
            Some(ToolResultEvent {
                tool_call_id: "c".into(),
                status: "completed".into(),
                diff: Some(diff),
                ..ToolResultEvent::default()
            }),
        )
    }

    #[test]
    fn edit_diff_counts_changed_lines_only() {
        let d = parse_diff(
            &json!({"file_path": "a.rs", "old_string": "a\nb\nc", "new_string": "a\nB\nc\nd"}),
        )
        .expect("diff");
        assert_eq!((d.added(), d.removed()), (2, 1));
        assert!(!d.is_write);
    }

    #[test]
    fn write_counts_every_line_as_added() {
        let d =
            parse_diff(&json!({"file_path": "n.rs", "new_content": "1\n2\n3\n"})).expect("diff");
        assert_eq!((d.added(), d.removed(), d.is_write), (3, 0, true));
    }

    #[test]
    fn no_diff_without_a_path_or_content() {
        assert!(parse_diff(&json!({"old_string": "x"})).is_none());
        assert!(parse_diff(&json!({"file_path": "a"})).is_none());
    }

    #[test]
    fn changed_files_aggregate_per_path_in_first_seen_order() {
        let tools = [
            edit("b.rs", "x", "y"),
            edit("a.rs", "1", "2\n3"),
            edit("b.rs", "p", "q"),
        ];
        let files = changed_files(&tools);
        assert_eq!(
            files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["b.rs", "a.rs"]
        );
        assert_eq!(
            (files[0].added, files[0].removed, files[0].edits),
            (2, 2, 2)
        );
        assert_eq!(totals(&files), (2, 4, 3));
    }

    #[test]
    fn failed_edits_are_not_counted() {
        let mut t = edit("a.rs", "x", "y");
        if let Some(o) = t.outcome.as_mut() {
            o.is_error = true;
        }
        assert!(changed_files(&[t]).is_empty());
    }

    #[test]
    fn side_by_side_pairs_removals_with_additions() {
        let lines = diff_lines("a\nb\nc\n", "a\nB\nB2\nc\n");
        let rows = side_by_side(&lines);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[1].0.as_ref().map(|l| l.text.as_str()), Some("b"));
        assert_eq!(rows[1].1.as_ref().map(|l| l.text.as_str()), Some("B"));
        assert!(rows[2].0.is_none());
        assert_eq!(rows[2].1.as_ref().map(|l| l.text.as_str()), Some("B2"));
    }
}
