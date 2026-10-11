//! Fixture conformance: a mock Pando serves the recorded bytes and the typed client must
//! reproduce them (SSE re-encoding is byte-identical) and decode them without Unknown events.

mod common;

use axum::Router;
use axum::http::header;
use axum::routing::{get, post};
use common::spawn_http;
use pando_client::{ChatEvent, PandoClient, PandoConfig, SseParser};

fn sse(body: &'static [u8]) -> impl axum::response::IntoResponse {
    ([(header::CONTENT_TYPE, "text/event-stream")], body)
}

fn json(body: &'static str) -> impl axum::response::IntoResponse {
    ([(header::CONTENT_TYPE, "application/json")], body)
}

fn mock() -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { json(include_str!("fixtures/health.json")) }),
        )
        .route(
            "/api/v1/token",
            get(|| async { json(include_str!("fixtures/token.json")) }),
        )
        .route(
            "/api/v1/models",
            get(|| async { json(include_str!("fixtures/models.json")) }),
        )
        .route(
            "/api/v1/sessions",
            get(|| async { json(include_str!("fixtures/sessions_list.json")) }),
        )
        .route(
            "/api/v1/orchestrator/tasks",
            get(|| async { json(include_str!("fixtures/orchestrator_tasks.json")) }),
        )
        .route(
            "/api/v1/chat/stream",
            post(|| async { sse(include_bytes!("fixtures/chat_tool_read.sse")) }),
        )
        .route(
            "/api/v1/sessions/{id}",
            get(|| async { json(include_str!("fixtures/session_detail.json")) }),
        )
        .route(
            "/api/v1/sessions/{id}/pending",
            get(|| async { json(include_str!("fixtures/session_pending.json")) }),
        )
        .route(
            "/api/v1/sessions/{id}/stream",
            get(|| async { sse(include_bytes!("fixtures/session_reattach.sse")) }),
        )
}

fn reencode(raw: &[u8]) -> Vec<u8> {
    let mut p = SseParser::new();
    let mut evs = p.push(raw);
    evs.extend(p.finish());
    let mut out = Vec::new();
    for e in evs {
        out.extend_from_slice(format!("event: {}\ndata: {}\n\n", e.event, e.data).as_bytes());
    }
    out
}

#[test]
fn sse_reencoding_is_byte_identical_for_every_real_fixture() {
    for (name, raw) in [
        ("simple", &include_bytes!("fixtures/chat_simple.sse")[..]),
        ("tool", &include_bytes!("fixtures/chat_tool_read.sse")[..]),
        ("slash", &include_bytes!("fixtures/chat_slash.sse")[..]),
        ("goal", &include_bytes!("fixtures/chat_goal_status.sse")[..]),
        (
            "reattach",
            &include_bytes!("fixtures/session_reattach.sse")[..],
        ),
        (
            "synthetic",
            &include_bytes!("fixtures/synthetic_events.sse")[..],
        ),
    ] {
        assert_eq!(reencode(raw), raw, "{name}");
    }
}

#[tokio::test]
async fn typed_calls_decode_the_recorded_responses() {
    let srv = spawn_http(mock()).await;
    let c = PandoClient::new(PandoConfig::new(srv.base_url())).unwrap_or_else(|e| panic!("{e}"));

    let info = c.info().await.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(info.status, "healthy");
    assert!(c.fetch_api_token().await.is_ok());
    assert!(
        !c.list_models()
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .models
            .is_empty()
    );
    assert!(
        !c.list_sessions(100, 0)
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .sessions
            .is_empty()
    );
    assert_eq!(
        c.list_tasks().await.unwrap_or_else(|e| panic!("{e}")).total,
        0
    );

    let detail = c.get_session("any").await.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(detail.messages.len(), 6);
    let p = c.pending("any").await.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(p.run_seq, 1);

    // The live chat stream and the reattach replay carry the same events.
    let mut chat = c
        .chat_stream(None, "hi", None)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let mut a = Vec::new();
    while let Some(ev) = chat.next().await.unwrap_or_else(|e| panic!("{e}")) {
        assert!(!matches!(ev, ChatEvent::Unknown { .. }), "{ev:?}");
        a.push(ev);
    }
    let mut att = c
        .attach_stream("any")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let mut b = Vec::new();
    while let Some(ev) = att.next().await.unwrap_or_else(|e| panic!("{e}")) {
        b.push(ev);
    }
    let content = |v: &[ChatEvent]| -> String {
        v.iter()
            .filter_map(|e| match e {
                ChatEvent::ContentDelta(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(content(&a), content(&b));
    assert!(!content(&a).is_empty());
}
