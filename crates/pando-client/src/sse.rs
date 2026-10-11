//! Incremental Server-Sent Events parser that keeps event names.
//!
//! Pando writes `event: <name>\ndata: <json>\n\n` (no `id:`, no `retry:`, no keep-alives).
//! Based on the `SseParser` of Bitácora's `pando-rs` (MIT, Copyright (c) 2026 Jose F. Rives
//! Lirola), extended to return the event name and id.

/// One complete SSE frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SseEvent {
    /// The `event:` name; empty for an unnamed (`message`) event.
    pub event: String,
    /// The `data:` lines joined with `\n`.
    pub data: String,
    /// The `id:` field, when present.
    pub id: Option<String>,
}

/// Reassembles SSE frames from arbitrary byte chunks (also split inside a UTF-8 sequence).
#[derive(Debug, Default)]
pub struct SseParser {
    line: Vec<u8>,
    event: String,
    data: Vec<String>,
    id: Option<String>,
}

impl SseParser {
    /// A fresh parser.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds a chunk; returns every frame it completed. Comments (`: ...`) are ignored.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        let mut out = Vec::new();
        for &byte in chunk {
            if byte == b'\n' {
                let mut line = std::mem::take(&mut self.line);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                self.handle_line(&line, &mut out);
            } else {
                self.line.push(byte);
            }
        }
        out
    }

    /// Flushes a trailing frame of a stream cut without its final blank line.
    pub fn finish(&mut self) -> Vec<SseEvent> {
        let mut out = Vec::new();
        if !self.line.is_empty() {
            let mut line = std::mem::take(&mut self.line);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.handle_line(&line, &mut out);
        }
        self.dispatch(&mut out);
        out
    }

    fn handle_line(&mut self, line: &[u8], out: &mut Vec<SseEvent>) {
        if line.is_empty() {
            self.dispatch(out);
            return;
        }
        if line.first() == Some(&b':') {
            return;
        }
        let text = String::from_utf8_lossy(line);
        let (field, value) = match text.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (text.as_ref(), ""),
        };
        match field {
            "event" => self.event = value.to_owned(),
            "data" => self.data.push(value.to_owned()),
            "id" => self.id = Some(value.to_owned()),
            _ => {}
        }
    }

    fn dispatch(&mut self, out: &mut Vec<SseEvent>) {
        let event = std::mem::take(&mut self.event);
        let id = self.id.take();
        if self.data.is_empty() {
            return;
        }
        let data = std::mem::take(&mut self.data).join("\n");
        out.push(SseEvent { event, data, id });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frames_across_chunks_and_ignores_comments() {
        let mut p = SseParser::new();
        assert!(p.push(b": hi\n\nevent: a\ndata: {\"a\"").is_empty());
        let got = p.push(b":1}\n\ndata:{\"b\":2}\r\n\r\n");
        assert_eq!(got.len(), 2);
        assert_eq!(
            (got[0].event.as_str(), got[0].data.as_str()),
            ("a", "{\"a\":1}")
        );
        assert_eq!(
            (got[1].event.as_str(), got[1].data.as_str()),
            ("", "{\"b\":2}")
        );
    }

    #[test]
    fn joins_multiline_data_flushes_tail_and_keeps_id() {
        let mut p = SseParser::new();
        let got = p.push(b"id: 7\nevent: x\ndata: l1\ndata: l2\n\ndata: tail");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].data, "l1\nl2");
        assert_eq!(got[0].id.as_deref(), Some("7"));
        let tail = p.finish();
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].data, "tail");
        assert_eq!(tail[0].event, "");
    }

    #[test]
    fn utf8_split_across_chunks_is_preserved() {
        let bytes = "data: \u{1f9e0} ok\n\n".as_bytes();
        let (a, b) = bytes.split_at(8); // inside the 4-byte emoji
        let mut p = SseParser::new();
        assert!(p.push(a).is_empty());
        let got = p.push(b);
        assert_eq!(got[0].data, "\u{1f9e0} ok");
    }

    #[test]
    fn replays_real_fixture_in_tiny_chunks() {
        let raw = include_bytes!("../tests/fixtures/chat_simple.sse");
        let mut p = SseParser::new();
        let mut names = Vec::new();
        for chunk in raw.chunks(7) {
            names.extend(p.push(chunk).into_iter().map(|e| e.event));
        }
        names.extend(p.finish().into_iter().map(|e| e.event));
        assert_eq!(names.first().map(String::as_str), Some("session"));
        assert_eq!(names.last().map(String::as_str), Some("done"));
        assert!(names.iter().any(|n| n == "content_delta"));
    }
}
