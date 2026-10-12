//! Loopback HTTP hook receiver (`POST /claude/hook`).
//!
//! Security: binds `127.0.0.1` only, random port, shared-secret header checked in constant
//! time *before* the body is read, bounded body size, bounded event queue. The secret is
//! never logged.

use crate::events::{HookEvent, MessagingEndpoint};
use crate::runtime::{RuntimeFiles, SECRET_HEADER, SOCKET_HEADER, TOKEN_HEADER, generate_secret};
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::post,
};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use subtle::ConstantTimeEq;

/// Maximum accepted hook body (Stop payloads carry the last assistant message).
pub const MAX_BODY_BYTES: usize = 256 * 1024;
/// Pending events before the receiver answers 503.
pub const QUEUE_CAPACITY: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum ReceiverError {
    #[error("could not bind loopback listener: {0}")]
    Bind(std::io::Error),
    #[error("could not write runtime files: {0}")]
    Runtime(std::io::Error),
    #[error("could not generate secret: {0}")]
    Random(String),
    #[error("server error: {0}")]
    Serve(std::io::Error),
}

#[derive(Clone)]
struct AppState {
    secret: Arc<str>,
    tx: async_channel::Sender<HookEvent>,
}

/// A bound (not yet serving) hook receiver. Runtime files are written on [`HookServer::bind`]
/// and removed when serving ends or the value is dropped.
pub struct HookServer {
    listener: tokio::net::TcpListener,
    state: AppState,
    rx: async_channel::Receiver<HookEvent>,
    runtime: RuntimeFiles,
}

impl std::fmt::Debug for HookServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookServer")
            .field("addr", &self.listener.local_addr().ok())
            .finish_non_exhaustive()
    }
}

impl HookServer {
    /// Bind `127.0.0.1:0`, generate a secret and write the runtime files.
    pub async fn bind(runtime: RuntimeFiles) -> Result<Self, ReceiverError> {
        let secret = generate_secret().map_err(|e| ReceiverError::Random(e.to_string()))?;
        let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .map_err(ReceiverError::Bind)?;
        let port = listener.local_addr().map_err(ReceiverError::Bind)?.port();
        runtime
            .write(port, &secret)
            .map_err(ReceiverError::Runtime)?;
        let (tx, rx) = async_channel::bounded(QUEUE_CAPACITY);
        Ok(Self {
            listener,
            state: AppState {
                secret: secret.into(),
                tx,
            },
            rx,
            runtime,
        })
    }

    /// The bound port.
    pub fn port(&self) -> u16 {
        self.listener.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    /// Stream of authenticated, parsed hook events (clone freely).
    pub fn events(&self) -> async_channel::Receiver<HookEvent> {
        self.rx.clone()
    }

    /// Serve until `shutdown` completes. Run this on the app's tokio bridge.
    pub async fn serve(
        self,
        shutdown: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), ReceiverError> {
        let app = router(self.state.clone());
        let res = axum::serve(self.listener, app)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(ReceiverError::Serve);
        self.runtime.remove();
        res
    }
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/claude/hook", post(hook))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .with_state(state)
}

/// Rejects with 401 before any body is read.
async fn authenticate(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let ok = req
        .headers()
        .get(SECRET_HEADER)
        .map(|v| bool::from(v.as_bytes().ct_eq(state.secret.as_bytes())))
        .unwrap_or(false);
    if !ok {
        return status(StatusCode::UNAUTHORIZED);
    }
    next.run(req).await
}

fn status(code: StatusCode) -> Response {
    let mut r = Response::new(axum::body::Body::empty());
    *r.status_mut() = code;
    r
}

fn messaging_from(headers: &HeaderMap) -> Option<MessagingEndpoint> {
    let socket = headers.get(SOCKET_HEADER)?.to_str().ok()?.trim();
    if socket.is_empty() || !socket.starts_with('/') || socket.len() > 4096 {
        return None;
    }
    let token = headers
        .get(TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|t| !t.is_empty() && t.len() <= 4096)
        .map(str::to_owned);
    Some(MessagingEndpoint {
        socket: socket.into(),
        token,
    })
}

async fn hook(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    match HookEvent::parse(&body, messaging_from(&headers)) {
        Ok(ev) => match state.tx.try_send(ev) {
            Ok(()) => StatusCode::NO_CONTENT,
            Err(_) => StatusCode::SERVICE_UNAVAILABLE,
        },
        // Events we do not model are accepted and dropped, so newer Claude Code versions or
        // extra registered hooks never produce errors in the user's session.
        Err(crate::events::ParseError::UnsupportedEvent(_)) => StatusCode::ACCEPTED,
        Err(e) => {
            tracing::debug!(error = %e, "rejected hook payload");
            StatusCode::BAD_REQUEST
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn post_raw(port: u16, headers: &[(&str, &str)], body: &[u8]) -> u16 {
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let mut req = format!(
            "POST /claude/hook HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");
        s.write_all(req.as_bytes()).await.unwrap();
        let _ = s.write_all(body).await;
        let mut out = String::new();
        let _ = s.read_to_string(&mut out).await;
        out.split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap_or(0)
    }

    fn secret_of(rt: &RuntimeFiles) -> String {
        let h = std::fs::read_to_string(rt.headers_file()).unwrap();
        h.trim().split(": ").nth(1).unwrap().to_owned()
    }

    #[tokio::test]
    async fn auth_size_and_forwarding() {
        let tmp = tempfile::tempdir().unwrap();
        let rt = RuntimeFiles::new(tmp.path().join("rt"));
        let server = HookServer::bind(rt.clone()).await.unwrap();
        let port = server.port();
        assert_eq!(rt.read_port(), Some(port));
        let secret = secret_of(&rt);
        let events = server.events();
        let (stop, rx) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(server.serve(async move {
            let _ = rx.await;
        }));

        let body = br#"{"session_id":"s1","cwd":"/p","hook_event_name":"SessionStart","source":"startup"}"#;
        // Missing / wrong secret => 401, nothing forwarded.
        assert_eq!(post_raw(port, &[], body).await, 401);
        assert_eq!(post_raw(port, &[(SECRET_HEADER, "wrong")], body).await, 401);
        assert!(events.is_empty());
        // Valid, with messaging headers.
        let hs = [
            (SECRET_HEADER, secret.as_str()),
            (SOCKET_HEADER, "/tmp/cc-socks-1/a.sock"),
            (TOKEN_HEADER, "tok"),
        ];
        assert_eq!(post_raw(port, &hs, body).await, 204);
        let ev = events.recv().await.unwrap();
        assert_eq!(ev.session_id, "s1");
        assert_eq!(ev.messaging.unwrap().token.as_deref(), Some("tok"));
        // Oversized body => 413; garbage => 400; unmodelled event => 202.
        let big = vec![b'x'; MAX_BODY_BYTES + 10];
        let auth = [(SECRET_HEADER, secret.as_str())];
        assert_eq!(post_raw(port, &auth, &big).await, 413);
        assert_eq!(post_raw(port, &auth, b"{").await, 400);
        assert_eq!(
            post_raw(
                port,
                &auth,
                br#"{"session_id":"s","hook_event_name":"PostToolUse"}"#
            )
            .await,
            202
        );
        assert!(events.is_empty());

        stop.send(()).unwrap();
        task.await.unwrap().unwrap();
        assert_eq!(rt.read_port(), None, "runtime files removed on shutdown");
    }
}
