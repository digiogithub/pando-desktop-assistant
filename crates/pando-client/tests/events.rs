//! Notification and config streams, decision-model reload on change.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::header;
use axum::routing::get;
use common::spawn_http;
use pando_client::events::{ConfigStreamEvent, NotificationEvent};
use pando_client::{EventStreamConfig, PandoClient, PandoConfig, StreamItem};

fn cfg() -> EventStreamConfig {
    EventStreamConfig {
        backoff_min: Duration::from_millis(10),
        backoff_max: Duration::from_millis(40),
        idle_timeout: None,
    }
}

fn sse(body: &'static str) -> impl axum::response::IntoResponse {
    ([(header::CONTENT_TYPE, "text/event-stream")], body)
}

#[tokio::test]
async fn notifications_are_decoded() {
    let app = Router::new().route(
        "/api/v1/notifications/stream",
        get(|| async {
            sse("event: connected\ndata: {}\n\nevent: notification\ndata: {\"id\":\"n1\",\"time\":\"2026-10-12T00:00:00Z\",\"level\":\"warn\",\"source\":\"tool\",\"message\":\"boom\",\"ttl\":5000000000}\n\nevent: weird\ndata: x\n\n")
        }),
    );
    let srv = spawn_http(app).await;
    let c = PandoClient::new(PandoConfig::new(srv.base_url())).unwrap_or_else(|e| panic!("{e}"));
    let mut s = c.notifications(cfg());
    let mut got = Vec::new();
    while got.len() < 3 {
        if let Some(StreamItem::Event(e)) = tokio::time::timeout(Duration::from_secs(5), s.next())
            .await
            .unwrap_or_else(|_| panic!("timeout"))
        {
            got.push(e);
        }
    }
    assert_eq!(got[0], NotificationEvent::Connected);
    assert!(
        matches!(&got[1], NotificationEvent::Notification(n) if n.level == "warn" && n.ttl == 5_000_000_000 && n.message == "boom")
    );
    assert!(matches!(&got[2], NotificationEvent::Unknown { event, .. } if event == "weird"));
}

#[derive(Clone)]
struct St {
    config_fetches: Arc<AtomicUsize>,
}

async fn config(State(s): State<St>) -> String {
    let n = s.config_fetches.fetch_add(1, Ordering::SeqCst);
    format!(
        r#"{{"router":{{"provider":"ollama","baseURL":"http://x","effectiveBaseURL":"http://x","model":"m{n}","apiKeySet":false,"apiKeyMasked":""}},"timeoutMs":0,"warnings":[]}}"#
    )
}

#[tokio::test]
async fn decision_model_change_event_triggers_reload() {
    // First connection: handshake only (initial load). Second: a decisionModel change, preceded by
    // an unrelated section that must NOT reload.
    let conn = Arc::new(AtomicUsize::new(0));
    let c2 = conn.clone();
    let st = St {
        config_fetches: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route(
            "/api/v1/config/events",
            get(move || {
                let c2 = c2.clone();
                async move {
                    if c2.fetch_add(1, Ordering::SeqCst) == 0 {
                        sse("event: connected\ndata: {\"ts\":1}\n\n")
                    } else {
                        sse("event: connected\ndata: {\"ts\":2}\n\ndata: {\"section\":\"agents\",\"timestamp\":\"t\",\"source\":\"webui\"}\n\ndata: {\"section\":\"decisionModel\",\"timestamp\":\"t\",\"source\":\"webui\"}\n\n")
                    }
                }
            }),
        )
        .route("/api/v1/config/decision-model", get(config))
        .with_state(st.clone());
    let srv = spawn_http(app).await;
    let client =
        PandoClient::new(PandoConfig::new(srv.base_url())).unwrap_or_else(|e| panic!("{e}"));
    let mut w = client.watch_decision_model(cfg());
    let mut models = Vec::new();
    // initial load + reload on reconnect + reload on the decisionModel change (not on "agents")
    while models.len() < 3 {
        let cfg = tokio::time::timeout(Duration::from_secs(5), w.next())
            .await
            .unwrap_or_else(|_| panic!("timeout"))
            .unwrap_or_else(|| panic!("closed"))
            .unwrap_or_else(|e| panic!("{e}"));
        models.push(cfg.router.model);
    }
    assert_eq!(models[0], "m0");
    assert!(models[1] > models[0] && models[2] > models[1]);
    let _ = ConfigStreamEvent::Connected;
}

#[test]
fn health_report_parses() {
    let h: pando_client::events::RouterHealth = serde_json::from_str(
        r#"{"ok":true,"report":{"ok":true,"kind":"ollama","model":"m","reachable":true,"authorized":true,"versionOK":true,"modelPresent":true,"isDecisionModel":true,"remote":false,"latencyMs":12,"problems":[]},"problems":[]}"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(h.ok && h.report.version_ok && h.report.latency_ms == 12);
}
