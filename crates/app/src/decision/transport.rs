//! reqwest `DecisionTransport` for `POST {effectiveBaseURL}/v1/systemone` (PDA-T-0097).
//!
//! The endpoint, model and `keep_alive` come from `GET /config/decision-model` (Pando never
//! returns the key itself). When a key is available in the environment it is sent as
//! `Authorization: Bearer`; it is never logged and never appears in an error message or in
//! `Debug` output.

use std::time::Duration;

use decide::transport::{TransportError, TransportRequest, TransportResponse};
use decide::{ClientOptions, DecisionTransport};
use pando_client::events::DecisionModelConfig;

/// Environment variables searched for the decision-model API key, in order.
pub const KEY_ENV_VARS: [&str; 2] = ["PANDO_DECISION_API_KEY", "PANDO_DECISION_MODEL_API_KEY"];

/// Responses larger than this are refused.
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// A secret that never prints.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Wraps a secret.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Where and how to call the decision model.
#[derive(Debug, Clone)]
pub struct DecisionEndpoint {
    /// Full URL of `/v1/systemone`.
    pub url: String,
    /// Model name.
    pub model: String,
    /// Bearer key, when one is configured in the environment.
    pub key: Option<Secret>,
    /// Ollama `keep_alive` is sent only to local providers.
    pub local: bool,
}

impl DecisionEndpoint {
    /// Builds the endpoint from Pando's config. `None` when no base URL is configured.
    #[must_use]
    pub fn from_config(
        cfg: &DecisionModelConfig,
        env: impl Fn(&str) -> Option<String>,
    ) -> Option<Self> {
        let base = [&cfg.router.effective_base_url, &cfg.router.base_url]
            .into_iter()
            .find(|b| !b.trim().is_empty())?;
        let base = base.trim().trim_end_matches('/');
        let key = KEY_ENV_VARS
            .iter()
            .find_map(|name| env(name))
            .filter(|k| !k.trim().is_empty())
            .map(|k| Secret(k.trim().to_owned()));
        Some(Self {
            url: format!("{base}/v1/systemone"),
            model: cfg.router.model.clone(),
            key,
            local: cfg.router.provider.eq_ignore_ascii_case("ollama"),
        })
    }

    /// Client options for this endpoint (remote gateways get no `keep_alive`).
    #[must_use]
    pub fn options(&self) -> ClientOptions {
        let mut o = ClientOptions::new(self.model.clone());
        if !self.local {
            o.keep_alive = None;
        }
        o
    }
}

/// The HTTP transport.
#[derive(Debug, Clone)]
pub struct HttpDecisionTransport {
    http: reqwest::Client,
    endpoint: DecisionEndpoint,
}

impl HttpDecisionTransport {
    /// Builds the transport (rustls, no redirects, no proxy surprises beyond the defaults).
    pub fn new(endpoint: DecisionEndpoint) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_millis(300))
            .build()?;
        Ok(Self { http, endpoint })
    }

    /// The endpoint.
    #[must_use]
    pub fn endpoint(&self) -> &DecisionEndpoint {
        &self.endpoint
    }
}

fn describe(err: reqwest::Error) -> String {
    // `without_url` keeps credentials embedded in a URL out of the text.
    let kind = if err.is_connect() {
        "connection refused or unreachable"
    } else if err.is_request() {
        "request failed"
    } else if err.is_body() || err.is_decode() {
        "bad response body"
    } else {
        "transport error"
    };
    let e = err.without_url();
    format!("{kind}: {e}")
}

impl DecisionTransport for HttpDecisionTransport {
    async fn post(&self, request: TransportRequest) -> Result<TransportResponse, TransportError> {
        let mut req = self
            .http
            .post(&self.endpoint.url)
            .timeout(request.timeout)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request.body);
        if let Some(Secret(key)) = &self.endpoint.key {
            req = req.bearer_auth(key);
        }
        let mut resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                TransportError::Timeout
            } else {
                TransportError::Unreachable(describe(e))
            }
        })?;
        let status = resp.status().as_u16();
        let mut body = Vec::new();
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
                        return Err(TransportError::Unreachable(
                            "response larger than 4 MiB".to_owned(),
                        ));
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) if e.is_timeout() => return Err(TransportError::Timeout),
                Err(e) => return Err(TransportError::Unreachable(describe(e))),
            }
        }
        Ok(TransportResponse { status, body })
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // real sockets need a real runtime in these tests
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::TcpListener;

    /// One-shot HTTP server: reads the request, replies with `reply`, returns the raw request.
    async fn serve(
        reply: &'static str,
        delay: Duration,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let h = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.expect("accept");
            let mut buf = vec![0u8; 16 * 1024];
            let mut got = Vec::new();
            loop {
                let n = s.read(&mut buf).await.expect("read");
                got.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&got).to_string();
                if let Some(i) = text.find("\r\n\r\n") {
                    let len = text[..i]
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if got.len() >= i + 4 + len {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            tokio::time::sleep(delay).await;
            let _ = s.write_all(reply.as_bytes()).await;
            let _ = s.shutdown().await;
            String::from_utf8_lossy(&got).to_string()
        });
        (format!("http://{addr}"), h)
    }

    fn cfg(base: &str, provider: &str) -> DecisionModelConfig {
        let mut c = DecisionModelConfig::default();
        c.router.effective_base_url = base.to_owned();
        c.router.model = "tev1:0.8b".into();
        c.router.provider = provider.into();
        c
    }

    fn request() -> TransportRequest {
        TransportRequest {
            body: br#"{"model":"m"}"#.to_vec(),
            timeout: Duration::from_millis(500),
        }
    }

    const OK: &str = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 14\r\nconnection: close\r\n\r\n{\"answers\":{}}";

    #[test]
    fn endpoint_comes_from_the_config_and_remote_has_no_keep_alive() {
        let e =
            DecisionEndpoint::from_config(&cfg("http://h:11434/", "ollama"), |_| None).expect("ep");
        assert_eq!(e.url, "http://h:11434/v1/systemone");
        assert!(e.options().keep_alive.is_some());
        let r =
            DecisionEndpoint::from_config(&cfg("https://gw/x", "custom"), |_| None).expect("ep");
        assert!(r.options().keep_alive.is_none());
        assert!(DecisionEndpoint::from_config(&DecisionModelConfig::default(), |_| None).is_none());
    }

    #[test]
    fn the_key_is_redacted_in_debug_output() {
        let e = DecisionEndpoint::from_config(&cfg("http://h", "ollama"), |n| {
            (n == "PANDO_DECISION_API_KEY").then(|| "sk-very-secret".to_owned())
        })
        .expect("ep");
        assert!(e.key.is_some());
        assert!(!format!("{e:?}").contains("sk-very-secret"));
    }

    #[tokio::test]
    async fn posts_json_with_bearer_and_returns_status_and_body() {
        let (base, server) = serve(OK, Duration::ZERO).await;
        let ep = DecisionEndpoint::from_config(&cfg(&base, "ollama"), |n| {
            (n == "PANDO_DECISION_API_KEY").then(|| "sk-abc".to_owned())
        })
        .expect("ep");
        let t = HttpDecisionTransport::new(ep).expect("client");
        let resp = t.post(request()).await.expect("ok");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, br#"{"answers":{}}"#);
        let seen = server.await.expect("server").to_lowercase();
        assert!(seen.starts_with("post /v1/systemone"), "{seen}");
        assert!(seen.contains("authorization: bearer sk-abc"));
        assert!(seen.contains("application/json"));
    }

    #[tokio::test]
    async fn http_errors_pass_through_as_statuses() {
        let (base, _s) = serve(
            "HTTP/1.1 401 Unauthorized\r\ncontent-length: 2\r\nconnection: close\r\n\r\nno",
            Duration::ZERO,
        )
        .await;
        let ep = DecisionEndpoint::from_config(&cfg(&base, "ollama"), |_| None).expect("ep");
        let resp = HttpDecisionTransport::new(ep)
            .expect("c")
            .post(request())
            .await
            .expect("resp");
        assert_eq!(resp.status, 401);
    }

    #[tokio::test]
    async fn a_slow_server_times_out() {
        let (base, _s) = serve(OK, Duration::from_millis(900)).await;
        let ep = DecisionEndpoint::from_config(&cfg(&base, "ollama"), |_| None).expect("ep");
        let err = HttpDecisionTransport::new(ep)
            .expect("c")
            .post(request())
            .await
            .expect_err("timeout");
        assert_eq!(err, TransportError::Timeout);
    }

    #[tokio::test]
    async fn a_closed_port_is_unreachable_without_leaking_the_key() {
        let ep = DecisionEndpoint::from_config(&cfg("http://127.0.0.1:9", "ollama"), |n| {
            (n == "PANDO_DECISION_API_KEY").then(|| "sk-secret-123".to_owned())
        })
        .expect("ep");
        let err = HttpDecisionTransport::new(ep)
            .expect("c")
            .post(request())
            .await
            .expect_err("down");
        match err {
            TransportError::Unreachable(m) => assert!(!m.contains("sk-secret-123"), "{m}"),
            TransportError::Timeout => {}
        }
    }

    #[tokio::test]
    async fn oversized_responses_are_refused() {
        let big: &'static str = Box::leak(
            format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                MAX_RESPONSE_BYTES + 10,
                "x".repeat(MAX_RESPONSE_BYTES + 10)
            )
            .into_boxed_str(),
        );
        let (base, _s) = serve(big, Duration::ZERO).await;
        let ep = DecisionEndpoint::from_config(&cfg(&base, "ollama"), |_| None).expect("ep");
        let mut req = request();
        req.timeout = Duration::from_secs(5);
        let err = HttpDecisionTransport::new(ep)
            .expect("c")
            .post(req)
            .await
            .expect_err("big");
        assert!(matches!(err, TransportError::Unreachable(_)));
    }
}
