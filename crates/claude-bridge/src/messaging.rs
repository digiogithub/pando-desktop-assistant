//! One-way cross-session messaging (PDA-T-0133), Linux/macOS only in v1.
//!
//! Documented facts (code.claude.com/docs/en/cross-session-messaging, 2026-10-11): connect to
//! the session's Unix socket (`CLAUDE_CODE_MESSAGING_SOCKET`), optionally send a first line
//! `{"type":"auth","token":"<CLAUDE_CODE_MESSAGING_TOKEN>"}`, then plain text; the connection
//! is closed if no complete line arrives within 30 s; ~1M chars cap.
//!
//! Everything about the *reply* the socket may send is undocumented. [`v1`] isolates that
//! guesswork: it only classifies what it can recognise and otherwise reports
//! [`Delivery::Sent`] (written, no verdict).

use crate::events::MessagingEndpoint;

/// Documented cap on message length, in characters.
pub const MAX_CHARS: usize = 1_000_000;

/// What we know about the outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// Written to the socket, no verdict from the receiver.
    Sent,
    /// The receiver answered with text we could not classify.
    SentWithReply(String),
    /// The receiver is holding the message until its user approves it.
    Held,
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("empty message")]
    Empty,
    #[error("message too long ({0} chars, max {MAX_CHARS})")]
    TooLong(usize),
    #[error("the session's messaging socket is not reachable: {0}")]
    Unreachable(std::io::Error),
    #[error("the session refused the message: {0}")]
    Refused(String),
    #[error("the session closed the connection before the message was written: {0}")]
    Closed(std::io::Error),
    #[error("cross-session messaging is only supported on Linux and macOS")]
    Unsupported,
}

/// Send `text` to a session. Blocking (call from a worker thread); bounded by short timeouts.
pub fn send_message(endpoint: &MessagingEndpoint, text: &str) -> Result<Delivery, SendError> {
    let text = text.trim_end_matches('\n');
    if text.trim().is_empty() {
        return Err(SendError::Empty);
    }
    let chars = text.chars().count();
    if chars > MAX_CHARS {
        return Err(SendError::TooLong(chars));
    }
    v1::send(endpoint, text)
}

#[cfg(unix)]
mod v1 {
    use super::{Delivery, SendError};
    use crate::events::MessagingEndpoint;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    const IO_TIMEOUT: Duration = Duration::from_secs(3);
    const REPLY_WAIT: Duration = Duration::from_millis(400);

    pub(super) fn send(ep: &MessagingEndpoint, text: &str) -> Result<Delivery, SendError> {
        let mut s = UnixStream::connect(&ep.socket).map_err(SendError::Unreachable)?;
        let _ = s.set_write_timeout(Some(IO_TIMEOUT));
        let mut payload = String::new();
        if let Some(token) = &ep.token {
            payload.push_str(&serde_json::json!({"type": "auth", "token": token}).to_string());
            payload.push('\n');
        }
        payload.push_str(text);
        payload.push('\n');
        s.write_all(payload.as_bytes()).map_err(SendError::Closed)?;
        let _ = s.flush();
        let _ = s.shutdown(std::net::Shutdown::Write);

        // Best effort: wait briefly for any verdict, never fail on read errors.
        let _ = s.set_read_timeout(Some(REPLY_WAIT));
        let mut buf = Vec::new();
        let _ = s.take(8192).read_to_end(&mut buf);
        classify(&String::from_utf8_lossy(&buf))
    }

    /// Heuristic classification of an undocumented reply.
    pub(super) fn classify(reply: &str) -> Result<Delivery, SendError> {
        let r = reply.trim();
        if r.is_empty() {
            return Ok(Delivery::Sent);
        }
        let lower = r.to_lowercase();
        if lower.contains("held") || lower.contains("hold") {
            Ok(Delivery::Held)
        } else if lower.contains("refus") || lower.contains("reject") || lower.contains("denied") {
            Err(SendError::Refused(r.to_owned()))
        } else {
            Ok(Delivery::SentWithReply(r.to_owned()))
        }
    }
}

#[cfg(not(unix))]
mod v1 {
    use super::{Delivery, SendError};
    use crate::events::MessagingEndpoint;
    pub(super) fn send(_: &MessagingEndpoint, _: &str) -> Result<Delivery, SendError> {
        Err(SendError::Unsupported)
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    fn serve_once(
        reply: Option<&'static str>,
    ) -> (
        tempfile::TempDir,
        std::path::PathBuf,
        std::thread::JoinHandle<Vec<String>>,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.sock");
        let l = UnixListener::bind(&path).unwrap();
        let h = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let lines: Vec<String> = BufReader::new(c.try_clone().unwrap())
                .lines()
                .map_while(Result::ok)
                .collect();
            if let Some(r) = reply {
                let _ = c.write_all(r.as_bytes());
            }
            lines
        });
        (tmp, path, h)
    }

    #[test]
    fn delivers_text_with_auth_line() {
        let (_t, path, h) = serve_once(None);
        let ep = MessagingEndpoint {
            socket: path,
            token: Some("tok".into()),
        };
        assert_eq!(
            send_message(&ep, "el schema cambió\n").unwrap(),
            Delivery::Sent
        );
        let lines = h.join().unwrap();
        assert!(lines[0].contains("\"auth\"") && lines[0].contains("tok"));
        assert_eq!(lines[1], "el schema cambió");
    }

    #[test]
    fn no_token_means_no_auth_line() {
        let (_t, path, h) = serve_once(None);
        let ep = MessagingEndpoint {
            socket: path,
            token: None,
        };
        send_message(&ep, "hola").unwrap();
        assert_eq!(h.join().unwrap(), vec!["hola".to_string()]);
    }

    #[test]
    fn classifies_held_reply() {
        let (_t, path, h) = serve_once(Some("message held for approval"));
        let ep = MessagingEndpoint {
            socket: path,
            token: None,
        };
        assert_eq!(send_message(&ep, "x").unwrap(), Delivery::Held);
        h.join().unwrap();
    }

    #[test]
    fn classifies_refusal() {
        let (_t, path, h) = serve_once(Some("inbound refused by user setting"));
        let ep = MessagingEndpoint {
            socket: path,
            token: None,
        };
        assert!(matches!(send_message(&ep, "x"), Err(SendError::Refused(_))));
        h.join().unwrap();
    }

    #[test]
    fn errors_are_reported() {
        let ep = MessagingEndpoint {
            socket: "/nonexistent/none.sock".into(),
            token: None,
        };
        assert!(matches!(
            send_message(&ep, "x"),
            Err(SendError::Unreachable(_))
        ));
        assert!(matches!(send_message(&ep, "  \n"), Err(SendError::Empty)));
        let big = "a".repeat(MAX_CHARS + 1);
        assert!(matches!(
            send_message(&ep, &big),
            Err(SendError::TooLong(_))
        ));
    }
}
