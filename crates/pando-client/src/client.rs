//! Authenticated HTTP transport for the Pando API.
//!
//! Derived from Bitácora's `pando-rs` client (MIT, Copyright (c) 2026 Jose F. Rives Lirola):
//! two `reqwest` clients (one with an overall timeout, one for SSE with only a per-read idle
//! timeout), a redacted token and a typed error, without the AG-UI part.

use std::collections::VecDeque;
use std::sync::{Arc, PoisonError, RwLock};

use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::auth::{AuthHooks, basic_account, decode_basic, encode_basic};
use crate::config::{BasicAuth, CLIENT_NAME, PandoConfig, Token};
use crate::error::{Error, Result};
use crate::sse::{SseEvent, SseParser};

/// What `GET /health` reports (unauthenticated; unknown fields are ignored).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ServerInfo {
    /// `healthy` when the server is up.
    pub status: String,
    /// Server version string, e.g. `v1.3.0`.
    pub version: String,
    /// Startup mode (`serve`, `desktop`, `project-child`, ...), empty when unset.
    pub startup_mode: String,
    /// Parent instance for a project child, empty otherwise.
    pub parent_instance_id: String,
    /// Active project id, empty when none.
    pub project_id: String,
    /// Active project name, empty when none.
    pub project_name: String,
    /// Public base path when served behind a prefix.
    pub public_base_path: String,
}

struct Inner {
    http: reqwest::Client,
    /// Same pool settings but no overall timeout, only a per-read idle timeout (SSE streams).
    stream_http: reqwest::Client,
    base: String,
    config: PandoConfig,
    token: RwLock<Option<Token>>,
    basic: RwLock<Option<BasicAuth>>,
    hooks: AuthHooks,
    /// Serialises token refreshes so a burst of 401s triggers one `GET /api/v1/token`.
    refresh_lock: tokio::sync::Mutex<()>,
    /// Serialises credential acquisition so a burst of 401s opens one dialog.
    basic_lock: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("base", &self.base)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Entry point of the client. Cheap to clone; clones share one connection pool and credentials.
#[derive(Debug, Clone)]
pub struct PandoClient {
    inner: Arc<Inner>,
}

impl PandoClient {
    /// Builds a client from `config`.
    pub fn new(config: PandoConfig) -> Result<Self> {
        Self::with_hooks(config, AuthHooks::default())
    }

    /// Builds a client with a credential store and/or login prompt for Basic auth.
    pub fn with_hooks(config: PandoConfig, hooks: AuthHooks) -> Result<Self> {
        let base = config.base_url.trim().trim_end_matches('/').to_owned();
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(Error::Config(format!(
                "base_url must start with http:// or https://, got {:?}",
                config.base_url
            )));
        }
        let roots = match &config.root_certificate_pem {
            Some(pem) => reqwest::Certificate::from_pem_bundle(pem)
                .map_err(|e| Error::Config(format!("invalid root certificate: {e}")))?,
            None => Vec::new(),
        };
        let with_roots = |mut b: reqwest::ClientBuilder| {
            for cert in &roots {
                b = b.add_root_certificate(cert.clone());
            }
            b
        };
        let http = with_roots(
            reqwest::Client::builder()
                .timeout(config.timeout)
                .connect_timeout(config.connect_timeout),
        )
        .build()
        .map_err(|e| Error::Config(e.to_string()))?;
        let stream_http = with_roots(
            reqwest::Client::builder()
                .read_timeout(config.stream_idle_timeout)
                .connect_timeout(config.connect_timeout),
        )
        .build()
        .map_err(|e| Error::Config(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                stream_http,
                base,
                token: RwLock::new(config.token.clone()),
                basic: RwLock::new(config.basic_auth.clone()),
                hooks,
                refresh_lock: tokio::sync::Mutex::new(()),
                basic_lock: tokio::sync::Mutex::new(()),
                config,
            }),
        })
    }

    /// The normalised base URL (no trailing slash).
    pub fn base_url(&self) -> &str {
        &self.inner.base
    }

    /// The configuration this client was built from (initial credentials included).
    pub fn config(&self) -> &PandoConfig {
        &self.inner.config
    }

    /// Replaces the token used by subsequent requests (shared by every clone).
    pub fn set_token(&self, token: Option<Token>) {
        *self
            .inner
            .token
            .write()
            .unwrap_or_else(PoisonError::into_inner) = token;
    }

    /// The token currently in use.
    pub fn token(&self) -> Option<Token> {
        self.inner
            .token
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Replaces the Basic credentials used by subsequent requests.
    pub fn set_basic_auth(&self, auth: Option<BasicAuth>) {
        *self
            .inner
            .basic
            .write()
            .unwrap_or_else(PoisonError::into_inner) = auth;
    }

    /// Server info from `GET /health` (unauthenticated on the server side).
    pub async fn info(&self) -> Result<ServerInfo> {
        self.get_json("/health").await
    }

    /// The API token of a local server from `GET /api/v1/token`. Pando answers it without
    /// credentials only on a loopback bind.
    pub async fn fetch_api_token(&self) -> Result<Token> {
        #[derive(Deserialize)]
        struct Body {
            token: String,
        }
        let body: Body = self.get_json("/api/v1/token").await?;
        Ok(Token::new(body.token))
    }

    /// `GET path` decoded as JSON.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send_json(self.request(Method::GET, path)).await
    }

    /// `POST path` with a JSON body, decoded as JSON.
    pub async fn post_json<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.send_json(self.request(Method::POST, path).json(body))
            .await
    }

    /// Opens an SSE stream: `POST path` with a JSON body (`Some`) or `GET path` (`None`).
    pub async fn open_sse<B: serde::Serialize>(
        &self,
        path: &str,
        body: Option<&B>,
    ) -> Result<SseStream> {
        let url = format!("{}{}", self.inner.base, path);
        let builder = match body {
            Some(b) => self.inner.stream_http.post(url).json(b),
            None => self.inner.stream_http.get(url),
        };
        let resp = self
            .send(
                self.authorize(builder)
                    .header("Accept", "text/event-stream"),
            )
            .await?;
        let is_sse = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"));
        if !is_sse {
            return Err(Error::Protocol(
                "expected a text/event-stream response".to_owned(),
            ));
        }
        Ok(SseStream::new(resp))
    }

    /// A request builder with the standard headers and credentials.
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.inner.base, path);
        self.authorize(self.inner.http.request(method, url))
    }

    fn authorize(&self, mut req: RequestBuilder) -> RequestBuilder {
        req = req.header("X-Pando-Client", CLIENT_NAME);
        if let Some(mut v) = self
            .token()
            .and_then(|t| reqwest::header::HeaderValue::from_str(t.expose()).ok())
        {
            v.set_sensitive(true);
            req = req.header("X-Pando-Token", v);
        }
        let basic = self
            .inner
            .basic
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(b) = basic {
            req = req.basic_auth(&b.username, Some(b.expose_password()));
        }
        req
    }

    /// Sends and decodes a JSON answer.
    pub async fn send_json<T: DeserializeOwned>(&self, req: RequestBuilder) -> Result<T> {
        let resp = self.send(req).await?;
        resp.json::<T>().await.map_err(Error::from_transport)
    }

    /// Sends the request and maps non-success statuses to [`Error`].
    ///
    /// Recovery, at most once each per call: a 401 refetches the ephemeral token from
    /// `/api/v1/token` (Pando regenerates it at every start) and retries; a
    /// `basic_auth_required` 401 obtains Basic credentials (keychain, then the login dialog)
    /// and retries. Requests whose body cannot be cloned are not retried.
    pub async fn send(&self, req: RequestBuilder) -> Result<Response> {
        self.send_with(req, true).await
    }

    async fn send_with(&self, req: RequestBuilder, refresh_token: bool) -> Result<Response> {
        let (http, request) = req.build_split();
        let request = request.map_err(Error::from_transport)?;
        let Some(template) = request.try_clone() else {
            return self.execute(&http, request).await;
        };
        let mut refreshed = !refresh_token;
        let mut basic_tries = 0;
        let mut request = request;
        loop {
            let used_token = self.token();
            let used_basic = self.current_basic();
            match self.execute(&http, request).await {
                Err(Error::Unauthorized) if !refreshed => {
                    refreshed = true;
                    match self.refresh_token(used_token).await {
                        Ok(()) => {}
                        // The token endpoint is unusable (no such route, bad body): the
                        // original rejection is the meaningful answer.
                        Err(
                            Error::Server { .. }
                            | Error::NotConfigured(_)
                            | Error::Decode(_)
                            | Error::Protocol(_)
                            | Error::Unauthorized,
                        ) => return Err(Error::Unauthorized),
                        Err(e) => return Err(e),
                    }
                }
                Err(Error::BasicAuthRequired) if basic_tries < 2 => {
                    basic_tries += 1;
                    self.acquire_basic(used_basic).await?;
                }
                other => return other,
            }
            let Some(mut next) = template.try_clone() else {
                return Err(Error::Unauthorized);
            };
            self.apply_credentials(&mut next);
            request = next;
        }
    }

    async fn execute(&self, http: &reqwest::Client, request: reqwest::Request) -> Result<Response> {
        let resp = http.execute(request).await.map_err(Error::from_transport)?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let body = resp.text().await.unwrap_or_default();
        Err(map_status(status, &body))
    }

    fn current_basic(&self) -> Option<BasicAuth> {
        self.inner
            .basic
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Overwrites the credential headers of `request` with the current credentials.
    fn apply_credentials(&self, request: &mut reqwest::Request) {
        use reqwest::header::{AUTHORIZATION, HeaderValue};
        let headers = request.headers_mut();
        match self
            .token()
            .and_then(|t| HeaderValue::from_str(t.expose()).ok())
        {
            Some(mut v) => {
                v.set_sensitive(true);
                headers.insert("X-Pando-Token", v);
            }
            None => {
                headers.remove("X-Pando-Token");
            }
        }
        match self.current_basic() {
            Some(b) => {
                let raw = b64(format!("{}:{}", b.username, b.expose_password()).as_bytes());
                if let Ok(mut v) = HeaderValue::from_str(&format!("Basic {raw}")) {
                    v.set_sensitive(true);
                    headers.insert(AUTHORIZATION, v);
                }
            }
            None => {
                headers.remove(AUTHORIZATION);
            }
        }
    }

    /// Refetches the token unless another caller already replaced `used`.
    async fn refresh_token(&self, used: Option<Token>) -> Result<()> {
        let _guard = self.inner.refresh_lock.lock().await;
        if self.token() != used {
            return Ok(());
        }
        tracing::debug!(base = %self.inner.base, "pando token rejected, refetching");
        #[derive(Deserialize)]
        struct Body {
            token: String,
        }
        let req = self.request(Method::GET, "/api/v1/token");
        let resp = Box::pin(self.send_with(req, false)).await?;
        let body: Body = resp.json().await.map_err(Error::from_transport)?;
        self.set_token(Some(Token::new(body.token)));
        Ok(())
    }

    /// Obtains Basic credentials unless another caller already changed them from `used`.
    async fn acquire_basic(&self, used: Option<BasicAuth>) -> Result<()> {
        let _guard = self.inner.basic_lock.lock().await;
        if self.current_basic() != used {
            return Ok(());
        }
        let account = basic_account(&self.inner.base);
        let store = self.inner.hooks.store.as_ref();
        if used.is_none() {
            if let Some(found) = store
                .and_then(|s| s.get(&account).ok().flatten())
                .and_then(|raw| decode_basic(&raw))
            {
                tracing::debug!(base = %self.inner.base, "using Basic credentials from the keychain");
                self.set_basic_auth(Some(found));
                return Ok(());
            }
        } else if let Some(s) = store {
            // The remembered credentials were rejected: forget them before asking again.
            let _ = s.delete(&account);
        }
        let Some(prompt) = self.inner.hooks.prompt.as_ref() else {
            return Err(Error::BasicAuthRequired);
        };
        tracing::debug!(base = %self.inner.base, "asking the user for Basic credentials");
        let Some(creds) = prompt.prompt(&self.inner.base).await else {
            return Err(Error::BasicAuthRequired);
        };
        if let Some(s) = store
            && let Err(e) = s.set(&account, &encode_basic(&creds))
        {
            tracing::warn!(error = %e, "could not save Basic credentials to the keychain");
        }
        self.set_basic_auth(Some(creds));
        Ok(())
    }
}

/// Standard base64 (RFC 4648) for the Basic header.
fn b64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for c in input.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(char::from(T[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn map_status(status: StatusCode, body: &str) -> Error {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned))
        .unwrap_or_else(|| body.chars().take(200).collect());
    match status {
        StatusCode::UNAUTHORIZED if message == "basic_auth_required" => Error::BasicAuthRequired,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Error::Unauthorized,
        StatusCode::SERVICE_UNAVAILABLE => Error::NotConfigured(message),
        _ => Error::Server {
            status: status.as_u16(),
            message,
        },
    }
}

/// A live SSE response. Pull events with [`SseStream::next_event`].
#[derive(Debug)]
pub struct SseStream {
    resp: Response,
    parser: SseParser,
    queue: VecDeque<SseEvent>,
    finished: bool,
}

impl SseStream {
    fn new(resp: Response) -> Self {
        Self {
            resp,
            parser: SseParser::new(),
            queue: VecDeque::new(),
            finished: false,
        }
    }

    /// The next event, `Ok(None)` once the body closed cleanly. A body that closes without a
    /// `done` event does not mean the run ended: reattach via `/sessions/{id}/stream`.
    pub async fn next_event(&mut self) -> Result<Option<SseEvent>> {
        loop {
            if let Some(ev) = self.queue.pop_front() {
                return Ok(Some(ev));
            }
            if self.finished {
                return Ok(None);
            }
            match self.resp.chunk().await.map_err(Error::from_transport)? {
                Some(chunk) => self.queue.extend(self.parser.push(&chunk)),
                None => {
                    self.finished = true;
                    self.queue.extend(self.parser.finish());
                }
            }
        }
    }
}
