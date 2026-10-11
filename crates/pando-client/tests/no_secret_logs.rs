//! Secret hygiene: logs and Debug output never contain tokens or passwords.
//! Own test binary because it installs a global tracing subscriber.

mod common;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use common::spawn_http;
use pando_client::auth::BoxFuture;
use pando_client::{
    AuthHooks, BasicAuth, CredentialPrompt, MemorySecretStore, PandoClient, PandoConfig,
};

struct Answer(BasicAuth);

impl CredentialPrompt for Answer {
    fn prompt<'a>(&'a self, _base_url: &'a str) -> BoxFuture<'a, Option<BasicAuth>> {
        Box::pin(async move { Some(self.0.clone()) })
    }
}

/// Captures every tracing event of the current thread into a buffer.
#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for Buf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn logs_and_debug_output_never_contain_secrets() {
    let buf = Buf::default();
    let sink = buf.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || sink.clone())
            .finish(),
    )
    .unwrap_or_else(|e| panic!("{e}"));

    // Server: Basic auth + token "TOKEN-SECRET-NEW"; the client starts with a stale token.
    let app = Router::new()
        .route(
            "/api/v1/token",
            get(|h: HeaderMap| async move {
                if h.contains_key("authorization") {
                    (StatusCode::OK, r#"{"token":"TOKEN-SECRET-NEW"}"#)
                } else {
                    (
                        StatusCode::UNAUTHORIZED,
                        r#"{"error":"basic_auth_required"}"#,
                    )
                }
            }),
        )
        .route(
            "/api/v1/ping",
            get(|h: HeaderMap| async move {
                if !h.contains_key("authorization") {
                    return (
                        StatusCode::UNAUTHORIZED,
                        r#"{"error":"basic_auth_required"}"#,
                    );
                }
                match h.get("x-pando-token").and_then(|v| v.to_str().ok()) {
                    Some("TOKEN-SECRET-NEW") => (StatusCode::OK, r#"{"ok":true}"#),
                    _ => (StatusCode::UNAUTHORIZED, r#"{"error":"unauthorized"}"#),
                }
            }),
        );
    let srv = spawn_http(app).await;
    let hooks = AuthHooks {
        store: Some(Arc::new(MemorySecretStore::default())),
        prompt: Some(Arc::new(Answer(BasicAuth::new("alice", "PASSWORD-SECRET")))),
    };
    let client = PandoClient::with_hooks(
        PandoConfig::new(srv.base_url()).with_token("TOKEN-SECRET-OLD"),
        hooks,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let _: serde_json::Value = client
        .get_json("/api/v1/ping")
        .await
        .unwrap_or_else(|e| panic!("{e}"));

    let mut text =
        String::from_utf8_lossy(&buf.0.lock().unwrap_or_else(|e| e.into_inner())).into_owned();
    text.push_str(&format!("{client:?} {:?}", client.token()));
    assert!(
        text.contains("refetching"),
        "expected the client's debug logs, got {text:?}"
    );
    assert!(text.contains("asking the user"), "expected the prompt log");
    for secret in ["TOKEN-SECRET-OLD", "TOKEN-SECRET-NEW", "PASSWORD-SECRET"] {
        assert!(!text.contains(secret), "{secret} leaked");
    }
}
