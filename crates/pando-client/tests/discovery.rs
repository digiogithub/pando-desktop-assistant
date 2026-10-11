//! Discovery against a temporary registry and mock http/https servers.

mod common;

use axum::Router;
use axum::routing::get;
use common::{TestCa, spawn_http, spawn_https};
use pando_client::{Discovery, Error};

fn health(mode: &'static str) -> Router {
    Router::new().route(
        "/health",
        get(move || async move {
            format!(r#"{{"status":"healthy","version":"v9.9.9","startup_mode":"{mode}"}}"#)
        }),
    )
}

fn write_entry(dir: &std::path::Path, id: &str, port: u16, mode: &str) {
    let body = format!(
        r#"{{"instance_id":"{id}","pid":1,"web_port":{port},"mode":"{mode}","is_primary":true}}"#
    );
    std::fs::write(dir.join(format!("{id}.json")), body).unwrap_or_else(|e| panic!("{e}"));
}

fn disc(dir: &std::path::Path) -> Discovery {
    Discovery {
        registry_dir: dir.to_owned(),
        host: "127.0.0.1".to_owned(),
        // a port nothing listens on, so the fallback never accidentally succeeds
        fallback_port: 1,
        ..Discovery::default()
    }
}

#[tokio::test]
async fn finds_https_server_from_registry_with_local_ca() {
    let ca = TestCa::new();
    let srv = spawn_https(&ca, health("serve")).await;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    write_entry(dir.path(), "a", srv.addr.port(), "webui");
    let mut d = disc(dir.path());
    d.root_certificate_pem = Some(ca.ca_pem.clone().into_bytes());
    let found = d.discover().await.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        found.base_url,
        format!("https://127.0.0.1:{}", srv.addr.port())
    );
    assert_eq!(found.info.version, "v9.9.9");
    assert_eq!(found.entry.map(|e| e.instance_id).as_deref(), Some("a"));
}

#[tokio::test]
async fn finds_http_server_for_desktop_mode() {
    let srv = spawn_http(health("desktop")).await;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    write_entry(dir.path(), "d", srv.addr.port(), "desktop");
    let found = disc(dir.path())
        .discover()
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(found.base_url.starts_with("http://"));
}

#[tokio::test]
async fn untrusted_https_falls_through_and_reports_unreachable() {
    let ca = TestCa::new();
    let srv = spawn_https(&ca, health("serve")).await;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    write_entry(dir.path(), "a", srv.addr.port(), "webui");
    // no CA configured: TLS verification must fail
    let err = disc(dir.path()).discover().await.err();
    assert!(matches!(err, Some(Error::Unreachable(_))), "{err:?}");
}

#[tokio::test]
async fn manual_url_overrides_registry_and_obeys_policy() {
    let srv = spawn_http(health("serve")).await;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let mut d = disc(dir.path());
    d.manual_url = Some(srv.base_url());
    assert_eq!(
        d.discover()
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .base_url,
        srv.base_url()
    );

    d.manual_url = Some("http://pando.example.com:1".to_owned());
    assert!(matches!(d.discover().await, Err(Error::Config(_))));
}

#[tokio::test]
async fn empty_registry_and_no_server_is_unreachable() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(
        disc(dir.path()).discover().await,
        Err(Error::Unreachable(_))
    ));
}

#[tokio::test]
async fn non_pando_health_answer_is_rejected() {
    let srv =
        spawn_http(Router::new().route("/health", get(|| async { r#"{"status":"nope"}"# }))).await;
    let d = disc(tempfile::tempdir().unwrap_or_else(|e| panic!("{e}")).path());
    assert!(matches!(
        d.probe(&srv.base_url()).await,
        Err(Error::Protocol(_))
    ));
}
