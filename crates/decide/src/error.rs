//! Typed errors of the System One client.
//!
//! The kinds mirror Pando's `internal/llm/systemone/errors.go` so logs and eval reports
//! read the same on both sides. Every variant is fail-open for the caller: an error means
//! "treat the transcript as conversation and grey out the decision chip"
//! (`PDA-SP-0008.R4`).

use thiserror::Error;

/// Failure of one `/v1/systemone` round trip.
///
/// Messages never contain the API key: the client never sees it (the transport owns
/// the `Authorization` header) and transports must redact it from their own errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecisionError {
    /// The backend could not be contacted (connection refused, DNS, TLS, ...).
    #[error("decision backend unreachable: {0}")]
    Unreachable(String),
    /// The per-call deadline elapsed.
    #[error("decision backend timed out")]
    Timeout,
    /// HTTP 401/403.
    #[error("decision backend refused the credentials")]
    Unauthorized,
    /// HTTP 404 (model not pulled / unknown).
    #[error("decision model not found: {0}")]
    ModelNotFound(String),
    /// HTTP 400, or a request that fails local validation.
    #[error("bad decision request: {0}")]
    BadRequest(String),
    /// HTTP 413, or the encoded body is above the 64 KiB cap.
    #[error("decision request too large: {0}")]
    TooLarge(String),
    /// HTTP 429.
    #[error("decision backend rate limited")]
    RateLimited,
    /// HTTP 5xx.
    #[error("decision backend error (HTTP {status}): {message}")]
    Server {
        /// HTTP status.
        status: u16,
        /// Server message, truncated.
        message: String,
    },
    /// The body is not valid JSON, an answer is missing, or a choice is not among the
    /// criteria of its question.
    #[error("malformed decision response: {0}")]
    Malformed(String),
}

impl DecisionError {
    /// Short stable identifier for logs and eval reports.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Unreachable(_) => "unreachable",
            Self::Timeout => "timeout",
            Self::Unauthorized => "unauthorized",
            Self::ModelNotFound(_) => "model_not_found",
            Self::BadRequest(_) => "bad_request",
            Self::TooLarge(_) => "too_large",
            Self::RateLimited => "rate_limited",
            Self::Server { .. } => "server",
            Self::Malformed(_) => "malformed",
        }
    }

    /// Map a non-2xx HTTP status (and its body) to a typed error, like Pando's
    /// `kindForStatus`.
    pub fn from_status(status: u16, body: &[u8]) -> Self {
        let message = extract_message(body);
        match status {
            400 => Self::BadRequest(message),
            401 | 403 => Self::Unauthorized,
            404 => Self::ModelNotFound(message),
            413 => Self::TooLarge(message),
            429 => Self::RateLimited,
            s if s >= 500 => Self::Server { status, message },
            _ => Self::BadRequest(format!("HTTP {status}: {message}")),
        }
    }
}

/// Pull `error` (string or `{message}`) out of a JSON error body, else the raw text,
/// truncated to 300 bytes on a char boundary.
fn extract_message(body: &[u8]) -> String {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
        match v.get("error") {
            Some(serde_json::Value::String(s)) => return s.clone(),
            Some(o) => {
                if let Some(m) = o.get("message").and_then(|m| m.as_str()) {
                    return m.to_string();
                }
            }
            None => {}
        }
    }
    let s = String::from_utf8_lossy(body);
    let s = s.trim();
    let mut end = s.len().min(300);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_matches_pando() {
        assert_eq!(DecisionError::from_status(401, b"").kind(), "unauthorized");
        assert_eq!(DecisionError::from_status(403, b"").kind(), "unauthorized");
        assert_eq!(DecisionError::from_status(400, b"").kind(), "bad_request");
        assert_eq!(
            DecisionError::from_status(404, b"").kind(),
            "model_not_found"
        );
        assert_eq!(DecisionError::from_status(413, b"").kind(), "too_large");
        assert_eq!(DecisionError::from_status(429, b"").kind(), "rate_limited");
        assert_eq!(DecisionError::from_status(503, b"").kind(), "server");
        assert_eq!(DecisionError::from_status(418, b"").kind(), "bad_request");
    }

    #[test]
    fn error_message_extraction() {
        assert_eq!(
            DecisionError::from_status(404, br#"{"error":"model 'x' not found"}"#),
            DecisionError::ModelNotFound("model 'x' not found".into())
        );
        assert_eq!(
            DecisionError::from_status(400, br#"{"error":{"message":"bad"}}"#),
            DecisionError::BadRequest("bad".into())
        );
        let long = "é".repeat(400);
        let DecisionError::ModelNotFound(m) = DecisionError::from_status(404, long.as_bytes())
        else {
            panic!("wrong kind");
        };
        assert!(m.len() <= 300);
    }
}
