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
        if let Some(token) = self.token() {
            req = req.header("X-Pando-Token", token.expose());
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

    /// Sends and maps non-success statuses to [`Error`].
    pub async fn send(&self, req: RequestBuilder) -> Result<Response> {
        let resp = req.send().await.map_err(Error::from_transport)?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let body = resp.text().await.unwrap_or_default();
        Err(map_status(status, &body))
    }
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
