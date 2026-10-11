//! Error type shared by every Pando API call.
//!
//! Derived from Bitácora's `pando-rs` (MIT, Copyright (c) 2026 Jose F. Rives Lirola).

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong talking to Pando.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 401/403: the token is missing or wrong.
    #[error("pando rejected the credentials")]
    Unauthorized,
    /// 401 with `basic_auth_required`: the server wants HTTP Basic credentials.
    #[error("pando requires basic authentication")]
    BasicAuthRequired,
    /// The server answered 503: the feature is not set up.
    #[error("pando feature not configured: {0}")]
    NotConfigured(String),
    /// The server could not be reached (connection refused, DNS, TLS).
    #[error("pando is unreachable: {0}")]
    Unreachable(String),
    /// The request timed out.
    #[error("pando request timed out")]
    Timeout,
    /// Any other non-success answer.
    #[error("pando server error {status}: {message}")]
    Server {
        /// HTTP status code.
        status: u16,
        /// Server-provided message (`{"error": ...}`) or the raw body prefix.
        message: String,
    },
    /// The response was not the JSON shape we expected.
    #[error("invalid pando response: {0}")]
    Decode(String),
    /// The server answered, but not with the protocol we expected (for example HTML from a
    /// proxy where an SSE stream was due).
    #[error("pando protocol error: {0}")]
    Protocol(String),
    /// The configuration is unusable (bad base URL, TLS init failure).
    #[error("invalid pando configuration: {0}")]
    Config(String),
}

impl Error {
    pub(crate) fn from_transport(err: reqwest::Error) -> Self {
        // `without_url` keeps query strings (which may carry `?token=`) out of messages.
        let err = err.without_url();
        if err.is_timeout() {
            Self::Timeout
        } else if err.is_decode() {
            Self::Decode(err.to_string())
        } else {
            Self::Unreachable(err.to_string())
        }
    }
}
