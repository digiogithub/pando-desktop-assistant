//! Client core against a local axum server: headers, error mapping, SSE fixture replay.

use axum::Router;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use pando_client::{BasicAuth, Error, PandoClient, PandoConfig, Token};

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let addr = listener.local_addr().unwrap_or_else(|e| panic!("{e}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

fn sse_response(body: &'static [u8]) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/event-stream")], body)
}

#[tokio::test]
async fn sends_standard_headers_and_decodes_health() {
    let app = Router::new().route(
        "/health",
        get(|h: HeaderMap| async move {
            assert_eq!(
                h.get("x-pando-client").map(|v| v.as_bytes()),
                Some(&b"pando-assistant"[..])
            );
            assert_eq!(
                h.get("x-pando-token").map(|v| v.as_bytes()),
                Some(&b"sekret"[..])
            );
            include_str!("fixtures/health.json").to_owned()
        }),
    );
    let base = serve(app).await;
    let client = PandoClient::new(PandoConfig::new(format!("{base}/")).with_token("sekret"))
        .unwrap_or_else(|e| panic!("{e}"));
    let info = client.info().await.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(info.status, "healthy");
    assert!(info.version.starts_with('v'));
}

#[tokio::test]
async fn maps_status_codes() {
    let app = Router::new()
        .route(
            "/a",
            get(|| async { (StatusCode::UNAUTHORIZED, r#"{"error":"unauthorized"}"#) }),
        )
        .route(
            "/b",
            get(|| async {
                (
                    StatusCode::UNAUTHORIZED,
                    r#"{"error":"basic_auth_required"}"#,
                )
            }),
        )
        .route(
            "/c",
            get(|| async { (StatusCode::SERVICE_UNAVAILABLE, r#"{"error":"kb off"}"#) }),
        )
        .route("/d", get(|| async { (StatusCode::BAD_REQUEST, "nope") }));
    let base = serve(app).await;
    let c = PandoClient::new(PandoConfig::new(base)).unwrap_or_else(|e| panic!("{e}"));
    let get = |p: &'static str| {
        let c = c.clone();
        async move { c.get_json::<serde_json::Value>(p).await }
    };
    assert!(matches!(get("/a").await, Err(Error::Unauthorized)));
    assert!(matches!(get("/b").await, Err(Error::BasicAuthRequired)));
    assert!(matches!(get("/c").await, Err(Error::NotConfigured(m)) if m == "kb off"));
    assert!(matches!(
        get("/d").await,
        Err(Error::Server { status: 400, .. })
    ));
}

#[tokio::test]
async fn basic_auth_header_is_sent() {
    let app = Router::new().route(
        "/x",
        get(|h: HeaderMap| async move {
            let v = h
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            format!("{{\"auth\":\"{v}\"}}")
        }),
    );
    let base = serve(app).await;
    let c = PandoClient::new(PandoConfig::new(base).with_basic_auth(BasicAuth::new("u", "p")))
        .unwrap_or_else(|e| panic!("{e}"));
    let v: serde_json::Value = c.get_json("/x").await.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(v["auth"], "Basic dTpw");
}

#[tokio::test]
async fn replays_real_chat_fixture_over_http() {
    let app = Router::new().route(
        "/api/v1/chat/stream",
        post(|| async { sse_response(include_bytes!("fixtures/chat_tool_read.sse")) }),
    );
    let base = serve(app).await;
    let c = PandoClient::new(PandoConfig::new(base)).unwrap_or_else(|e| panic!("{e}"));
    let mut s = c
        .open_sse(
            "/api/v1/chat/stream",
            Some(&serde_json::json!({"prompt": "x"})),
        )
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let mut names = Vec::new();
    while let Some(ev) = s.next_event().await.unwrap_or_else(|e| panic!("{e}")) {
        // every payload of every real event must be valid JSON
        assert!(
            serde_json::from_str::<serde_json::Value>(&ev.data).is_ok(),
            "bad json in {}",
            ev.event
        );
        names.push(ev.event);
    }
    assert_eq!(names.first().map(String::as_str), Some("session"));
    assert_eq!(names.last().map(String::as_str), Some("done"));
    assert!(names.iter().any(|n| n == "tool_call"));
    assert!(names.iter().any(|n| n == "tool_result"));
}

#[tokio::test]
async fn html_instead_of_sse_is_a_protocol_error() {
    let app = Router::new().route(
        "/s",
        get(|| async { ([(header::CONTENT_TYPE, "text/html")], "<html>") }),
    );
    let base = serve(app).await;
    let c = PandoClient::new(PandoConfig::new(base)).unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(
        c.open_sse::<()>("/s", None).await,
        Err(Error::Protocol(_))
    ));
}

#[test]
fn token_and_basic_auth_are_redacted_in_debug() {
    let cfg = PandoConfig::new("http://x")
        .with_token("topsecret")
        .with_basic_auth(BasicAuth::new("u", "pw123"));
    let dbg = format!("{cfg:?} {:?}", Token::new("topsecret"));
    assert!(!dbg.contains("topsecret"));
    assert!(!dbg.contains("pw123"));
    let client = PandoClient::new(cfg).unwrap_or_else(|e| panic!("{e}"));
    assert!(!format!("{client:?}").contains("topsecret"));
}

#[test]
fn rejects_bad_base_url() {
    assert!(matches!(
        PandoClient::new(PandoConfig::new("ftp://x")),
        Err(Error::Config(_))
    ));
}
