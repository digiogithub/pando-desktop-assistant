//! EventStream: protocol check, reconnection after a drop, backoff reset, watchdog.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::http::header;
use axum::routing::get;
use common::spawn_http;
use pando_client::{EventStream, EventStreamConfig, PandoClient, PandoConfig, StreamItem};

fn cfg() -> EventStreamConfig {
    EventStreamConfig {
        backoff_min: Duration::from_millis(10),
        backoff_max: Duration::from_millis(40),
        idle_timeout: None,
    }
}

async fn next<T: Send + 'static>(s: &mut EventStream<T>) -> StreamItem<T> {
    tokio::time::timeout(Duration::from_secs(5), s.next())
        .await
        .unwrap_or_else(|_| panic!("timeout"))
        .unwrap_or_else(|| panic!("closed"))
}

#[tokio::test]
async fn html_with_200_is_a_protocol_error_then_retried() {
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let app = Router::new().route(
        "/s",
        get(move || {
            let h = h.clone();
            async move {
                h.fetch_add(1, Ordering::SeqCst);
                ([(header::CONTENT_TYPE, "text/html")], "<html>")
            }
        }),
    );
    let srv = spawn_http(app).await;
    let c = PandoClient::new(PandoConfig::new(srv.base_url())).unwrap_or_else(|e| panic!("{e}"));
    let mut s = EventStream::<String>::spawn(c, "/s", cfg(), |e| Some(e.data.clone()));
    match next(&mut s).await {
        StreamItem::Disconnected { reason } => assert!(reason.contains("protocol"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        next(&mut s).await,
        StreamItem::Disconnected { .. }
    ));
    assert!(hits.load(Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn connection_drop_reconnects_and_keeps_delivering() {
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let app = Router::new().route(
        "/s",
        get(move || {
            let h = h.clone();
            async move {
                let n = h.fetch_add(1, Ordering::SeqCst);
                (
                    [(header::CONTENT_TYPE, "text/event-stream")],
                    format!("event: notification\ndata: {{\"n\":{n}}}\n\n"),
                )
            }
        }),
    );
    let srv = spawn_http(app).await;
    let c = PandoClient::new(PandoConfig::new(srv.base_url())).unwrap_or_else(|e| panic!("{e}"));
    let mut s = EventStream::<(String, String)>::spawn(c, "/s", cfg(), |e| {
        Some((e.event.clone(), e.data.clone()))
    });
    let mut got = Vec::new();
    while got.len() < 2 {
        if let StreamItem::Event((name, data)) = next(&mut s).await {
            assert_eq!(name, "notification");
            got.push(data);
        }
    }
    assert_eq!(got, vec![r#"{"n":0}"#, r#"{"n":1}"#]);
}

#[tokio::test]
async fn watchdog_reconnects_a_silent_stream() {
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    // An endless body that never sends anything after the first comment.
    let app = Router::new().route(
        "/s",
        get(move || {
            let h = h.clone();
            async move {
                h.fetch_add(1, Ordering::SeqCst);
                let stream = futures_stream();
                (
                    [(header::CONTENT_TYPE, "text/event-stream")],
                    axum::body::Body::from_stream(stream),
                )
            }
        }),
    );
    let srv = spawn_http(app).await;
    let c = PandoClient::new(PandoConfig::new(srv.base_url())).unwrap_or_else(|e| panic!("{e}"));
    let mut config = cfg();
    config.idle_timeout = Some(Duration::from_millis(100));
    let mut s = EventStream::<String>::spawn(c, "/s", config, |e| Some(e.data.clone()));
    assert_eq!(next(&mut s).await, StreamItem::Connected);
    match next(&mut s).await {
        StreamItem::Disconnected { reason } => assert_eq!(reason, "idle timeout"),
        other => panic!("{other:?}"),
    }
    assert_eq!(next(&mut s).await, StreamItem::Connected);
    assert!(hits.load(Ordering::SeqCst) >= 2);
}

/// Body stream: one chunk, then silence forever.
struct Silent(Option<&'static str>);

fn futures_stream() -> Silent {
    Silent(Some(": hi\n\n"))
}

impl futures_core::Stream for Silent {
    type Item = Result<bytes::Bytes, std::convert::Infallible>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.0.take() {
            Some(s) => std::task::Poll::Ready(Some(Ok(bytes::Bytes::from_static(s.as_bytes())))),
            None => std::task::Poll::Pending,
        }
    }
}
