//! ConnectionService against mock servers that stop and start.

mod common;

use std::time::Duration;

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use common::{TestServer, spawn_http, spawn_http_at};
use pando_client::{ConnectionOptions, ConnectionService, ConnectionState, Discovery, Version};

fn app(version: &'static str, require_token: Option<&'static str>) -> Router {
    Router::new()
        .route(
            "/health",
            get(move || async move {
                format!(
                    r#"{{"status":"healthy","version":"{version}","startup_mode":"serve","project_name":"demo"}}"#
                )
            }),
        )
        .route(
            "/api/v1/token",
            get(move || async move { format!(r#"{{"token":"{}"}}"#, require_token.unwrap_or("x")) }),
        )
        .route(
            "/api/v1/sessions",
            get(move |h: HeaderMap| async move {
                match require_token {
                    Some(t) if h.get("x-pando-token").and_then(|v| v.to_str().ok()) != Some(t) => {
                        (StatusCode::UNAUTHORIZED, r#"{"error":"unauthorized"}"#.to_owned())
                    }
                    _ => (StatusCode::OK, r#"{"sessions":[]}"#.to_owned()),
                }
            }),
        )
}

fn options(url: String) -> ConnectionOptions {
    ConnectionOptions {
        discovery: Discovery {
            manual_url: Some(url),
            ..Discovery::default()
        },
        health_interval: Duration::from_millis(80),
        backoff_min: Duration::from_millis(20),
        backoff_max: Duration::from_millis(160),
        ..ConnectionOptions::default()
    }
}

async fn wait_state(
    svc: &ConnectionService,
    pred: impl Fn(&ConnectionState) -> bool,
) -> ConnectionState {
    let mut rx = svc.subscribe();
    let fut = async {
        loop {
            let s = rx.borrow_and_update().state.clone();
            if pred(&s) {
                return s;
            }
            if rx.changed().await.is_err() {
                panic!("service stopped");
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), fut)
        .await
        .unwrap_or_else(|_| panic!("timeout; state = {:?}", svc.state()))
}

#[tokio::test]
async fn connects_reports_version_and_project() {
    let srv = spawn_http(app("v1.3.0", None)).await;
    let svc = ConnectionService::start(options(srv.base_url()));
    let s = wait_state(&svc, |s| matches!(s, ConnectionState::Connected { .. })).await;
    assert_eq!(
        s,
        ConnectionState::Connected {
            version: "v1.3.0".into(),
            project: Some("demo".into())
        }
    );
}

#[tokio::test]
async fn stop_and_restart_flips_state_and_reopens_streams() {
    let srv = spawn_http(app("v1.3.0", None)).await;
    let addr = srv.addr;
    let svc = ConnectionService::start(options(srv.base_url()));
    let mut link = svc.link().await;
    let gen1 = link.generation();

    // Pando stops: the link closes and the state becomes Unavailable.
    srv.stop();
    drop(srv);
    tokio::time::timeout(Duration::from_secs(5), link.closed())
        .await
        .unwrap_or_else(|_| panic!("link did not close"));
    wait_state(&svc, |s| matches!(s, ConnectionState::Unavailable { .. })).await;

    // Pando starts again on the same port: Connected with a new generation.
    let _srv2: TestServer = spawn_http_at(addr, app("v1.3.0", None)).await;
    let link2 = svc.link().await;
    assert!(link2.generation() > gen1);
    assert!(matches!(svc.state(), ConnectionState::Connected { .. }));
    let _: serde_json::Value = link2
        .client
        .get_json("/health")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
}

#[tokio::test]
async fn too_old_server_is_gated() {
    let srv = spawn_http(app("v0.9.1", None)).await;
    let mut o = options(srv.base_url());
    o.min_version = Version(1, 0, 0);
    let svc = ConnectionService::start(o);
    let s = wait_state(&svc, |s| matches!(s, ConnectionState::TooOld { .. })).await;
    assert_eq!(
        s,
        ConnectionState::TooOld {
            min: "1.0.0".into(),
            found: "v0.9.1".into()
        }
    );
}

#[tokio::test]
async fn rejected_credentials_report_unauthorized() {
    // The token endpoint hands out a token the sessions route rejects.
    let app = Router::new()
        .route(
            "/health",
            get(|| async { r#"{"status":"healthy","version":"v1.3.0"}"# }),
        )
        .route("/api/v1/token", get(|| async { r#"{"token":"x"}"# }))
        .route(
            "/api/v1/sessions",
            get(|| async { (StatusCode::UNAUTHORIZED, r#"{"error":"unauthorized"}"#) }),
        );
    let srv = spawn_http(app).await;
    let svc = ConnectionService::start(options(srv.base_url()));
    wait_state(&svc, |s| *s == ConnectionState::Unauthorized).await;
}

#[tokio::test]
async fn nothing_listening_is_unavailable_then_recovers_with_backoff() {
    // Reserve a port, then free it.
    let probe = spawn_http(Router::new()).await;
    let addr = probe.addr;
    drop(probe);
    let svc = ConnectionService::start(options(format!("http://{addr}")));
    wait_state(&svc, |s| matches!(s, ConnectionState::Unavailable { .. })).await;
    let _srv = spawn_http_at(addr, app("v1.3.0", None)).await;
    wait_state(&svc, |s| matches!(s, ConnectionState::Connected { .. })).await;
}

#[tokio::test]
async fn token_is_obtained_during_connect() {
    let srv = spawn_http(app("v1.3.0", Some("tok"))).await;
    let svc = ConnectionService::start(options(srv.base_url()));
    let link = svc.link().await;
    // The authenticated client works (token fetched on the first 401 during connect).
    let _: serde_json::Value = link
        .client
        .get_json("/api/v1/sessions?limit=1")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
}
