//! Typed Claude Code hook payloads.
//!
//! Format source: <https://code.claude.com/docs/en/hooks> (read 2026-10-12). Every hook gets
//! `session_id`, `cwd`, `hook_event_name`, `transcript_path` on stdin; event-specific fields
//! are optional here because the format evolves (unknown fields are ignored).

use serde::Deserialize;
use std::path::PathBuf;

/// The hook events the bridge understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookKind {
    SessionStart,
    UserPromptSubmit,
    PreToolUse,
    Notification,
    Stop,
    SessionEnd,
}

impl HookKind {
    /// Every kind the installer registers.
    pub const ALL: [HookKind; 6] = [
        HookKind::SessionStart,
        HookKind::UserPromptSubmit,
        HookKind::PreToolUse,
        HookKind::Notification,
        HookKind::Stop,
        HookKind::SessionEnd,
    ];

    /// The `hook_event_name` / settings key.
    pub fn name(self) -> &'static str {
        match self {
            HookKind::SessionStart => "SessionStart",
            HookKind::UserPromptSubmit => "UserPromptSubmit",
            HookKind::PreToolUse => "PreToolUse",
            HookKind::Notification => "Notification",
            HookKind::Stop => "Stop",
            HookKind::SessionEnd => "SessionEnd",
        }
    }

    /// Parse a `hook_event_name`.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// Where to post messages to a session (captured from the hook's environment by the
/// forwarder and passed as headers).
#[derive(Clone, PartialEq, Eq)]
pub struct MessagingEndpoint {
    /// Value of `CLAUDE_CODE_MESSAGING_SOCKET` (a Unix socket path).
    pub socket: PathBuf,
    /// Value of `CLAUDE_CODE_MESSAGING_TOKEN`, when available.
    pub token: Option<String>,
}

impl std::fmt::Debug for MessagingEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the token.
        f.debug_struct("MessagingEndpoint")
            .field("socket", &self.socket)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// One received hook event.
#[derive(Debug, Clone, PartialEq)]
pub struct HookEvent {
    pub kind: HookKind,
    pub session_id: String,
    pub cwd: PathBuf,
    /// `session_title` (set by `--name` / `/rename`), when present.
    pub session_title: Option<String>,
    /// SessionStart `source`: startup, resume, clear, compact, fork.
    pub source: Option<String>,
    /// Notification `message`.
    pub message: Option<String>,
    /// Notification `notification_type` (permission_prompt, idle_prompt, ...).
    pub notification_type: Option<String>,
    /// Stop `last_assistant_message`.
    pub last_assistant_message: Option<String>,
    /// UserPromptSubmit `prompt`.
    pub prompt: Option<String>,
    /// PreToolUse `tool_name`.
    pub tool_name: Option<String>,
    /// SessionEnd `reason`.
    pub reason: Option<String>,
    /// Set when the hook fired inside a subagent.
    pub agent_id: Option<String>,
    pub messaging: Option<MessagingEndpoint>,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("invalid hook JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported hook event {0:?}")]
    UnsupportedEvent(String),
    #[error("missing session_id")]
    MissingSession,
}

#[derive(Deserialize)]
struct Raw {
    hook_event_name: String,
    session_id: Option<String>,
    cwd: Option<String>,
    session_title: Option<String>,
    source: Option<String>,
    message: Option<String>,
    notification_type: Option<String>,
    last_assistant_message: Option<String>,
    prompt: Option<String>,
    tool_name: Option<String>,
    reason: Option<String>,
    agent_id: Option<String>,
}

impl HookEvent {
    /// Parse a hook body. `messaging` comes from the forwarder's headers.
    pub fn parse(body: &[u8], messaging: Option<MessagingEndpoint>) -> Result<Self, ParseError> {
        let raw: Raw = serde_json::from_slice(body)?;
        let kind = HookKind::parse(&raw.hook_event_name)
            .ok_or_else(|| ParseError::UnsupportedEvent(raw.hook_event_name.clone()))?;
        let session_id = raw
            .session_id
            .filter(|s| !s.is_empty())
            .ok_or(ParseError::MissingSession)?;
        Ok(Self {
            kind,
            session_id,
            cwd: raw.cwd.map(PathBuf::from).unwrap_or_default(),
            session_title: raw.session_title.filter(|s| !s.trim().is_empty()),
            source: raw.source,
            message: raw.message,
            notification_type: raw.notification_type,
            last_assistant_message: raw.last_assistant_message,
            prompt: raw.prompt,
            tool_name: raw.tool_name,
            reason: raw.reason,
            agent_id: raw.agent_id,
            messaging,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stop_payload() {
        let body = br#"{"session_id":"abc","cwd":"/home/u/p","hook_event_name":"Stop",
            "stop_hook_active":false,"last_assistant_message":"done","background_tasks":[]}"#;
        let ev = HookEvent::parse(body, None).unwrap();
        assert_eq!(ev.kind, HookKind::Stop);
        assert_eq!(ev.last_assistant_message.as_deref(), Some("done"));
        assert_eq!(ev.cwd, PathBuf::from("/home/u/p"));
    }

    #[test]
    fn rejects_unknown_event_and_missing_session() {
        assert!(matches!(
            HookEvent::parse(
                br#"{"session_id":"a","hook_event_name":"PostToolUse"}"#,
                None
            ),
            Err(ParseError::UnsupportedEvent(_))
        ));
        assert!(matches!(
            HookEvent::parse(br#"{"hook_event_name":"Stop"}"#, None),
            Err(ParseError::MissingSession)
        ));
        assert!(HookEvent::parse(b"nope", None).is_err());
    }

    #[test]
    fn debug_never_prints_token() {
        let e = MessagingEndpoint {
            socket: "/tmp/s".into(),
            token: Some("sekret".into()),
        };
        assert!(!format!("{e:?}").contains("sekret"));
    }
}
