//! Connection settings and credentials.
//!
//! Derived from Bitácora's `pando-rs` (MIT, Copyright (c) 2026 Jose F. Rives Lirola).

use std::fmt;
use std::time::Duration;

/// Default overall request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Default TCP connect timeout.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Longest silence tolerated on a streaming response. Pando sends **no** SSE keep-alives, so a
/// long tool run is silent; the default is generous and callers reattach via `/pending`.
pub const DEFAULT_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(300);
/// Value of the `X-Pando-Client` header (suppresses the `WWW-Authenticate` challenge).
pub const CLIENT_NAME: &str = "pando-assistant";

/// An API token. `Debug` never prints the secret.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// Wraps a raw token string.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The raw secret, for building the `X-Pando-Token` header.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

impl From<String> for Token {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Token {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// HTTP Basic credentials for servers with `[Server.BasicAuth]`. `Debug` hides the password.
#[derive(Clone, PartialEq, Eq)]
pub struct BasicAuth {
    /// User name.
    pub username: String,
    password: String,
}

impl BasicAuth {
    /// Wraps a user name and password.
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }

    /// The raw password, for building the `Authorization` header.
    pub fn expose_password(&self) -> &str {
        &self.password
    }
}

impl fmt::Debug for BasicAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BasicAuth")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// Where and how to reach a Pando server.
#[derive(Debug, Clone)]
pub struct PandoConfig {
    /// Server root, e.g. `https://localhost:8765`. A trailing slash is ignored.
    pub base_url: String,
    /// API token sent as `X-Pando-Token`. `None` sends no credential.
    pub token: Option<Token>,
    /// Basic credentials, sent as `Authorization: Basic` when set.
    pub basic_auth: Option<BasicAuth>,
    /// Overall per-request timeout.
    pub timeout: Duration,
    /// TCP connect timeout.
    pub connect_timeout: Duration,
    /// Longest silence tolerated on a streaming response.
    pub stream_idle_timeout: Duration,
    /// Extra root certificate(s) (PEM) trusted for HTTPS: the local CA of `pando serve`.
    pub root_certificate_pem: Option<Vec<u8>>,
}

impl PandoConfig {
    /// A config for `base_url` with default timeouts and no credentials.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            token: None,
            basic_auth: None,
            timeout: DEFAULT_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            stream_idle_timeout: DEFAULT_STREAM_IDLE_TIMEOUT,
            root_certificate_pem: None,
        }
    }

    /// Trusts `pem` (one or more PEM certificates) in addition to the system roots.
    pub fn with_root_certificate_pem(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.root_certificate_pem = Some(pem.into());
        self
    }

    /// Sets the API token.
    pub fn with_token(mut self, token: impl Into<Token>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Sets Basic credentials.
    pub fn with_basic_auth(mut self, auth: BasicAuth) -> Self {
        self.basic_auth = Some(auth);
        self
    }

    /// Sets the overall request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the connect timeout.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets the idle timeout of streaming responses.
    pub fn with_stream_idle_timeout(mut self, timeout: Duration) -> Self {
        self.stream_idle_timeout = timeout;
        self
    }
}
