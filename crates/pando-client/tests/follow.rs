//! run_seq protocol scenarios (PDA-SP-0002.R4) against a mock Pando.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::header;
use axum::routing::get;
use common::spawn_http;
use pando_client::follow::{FollowConfig, FollowEvent, follow_session, seen_for};
use pando_client::{ChatEvent, PandoClient, PandoConfig};

#[derive(Clone)]
struct World {
    run_seq: Arc<AtomicI64>,
    running: Arc<AtomicBool>,
    stream_hits: Arc<AtomicUsize>,
    /// Bodies served by successive `/stream` calls (the last one repeats).
    bodies: Arc<Vec<&'static str>>,
}

async fn pending(State(w): State<World>) -> String {
    format!(
        r#"{{"permissions":[],"questions":[],"running":{},"run_seq":{}}}"#,
        w.running.load(Ordering::SeqCst),
        w.run_seq.load(Ordering::SeqCst)
    )
}

async fn stream(State(w): State<World>) -> impl axum::response::IntoResponse {
    let n = w.stream_hits.fetch_add(1, Ordering::SeqCst);
    let body = w.bodies[n.min(w.bodies.len() - 1)];
    ([(header::CONTENT_TYPE, "text/event-stream")], body)
}

async fn session(State(w): State<World>) -> String {
    format!(
        r#"{{"session":{{"ID":"s1","Title":"t"}},"messages":[{{"ID":"m1","Role":"assistant","Parts":[{{"text":"from history"}}]}}],"is_running":false,"run_seq":{}}}"#,
        w.run_seq.load(Ordering::SeqCst)
    )
}

async fn setup(
    run_seq: i64,
    running: bool,
    bodies: Vec<&'static str>,
) -> (common::TestServer, World, PandoClient) {
    let w = World {
        run_seq: Arc::new(AtomicI64::new(run_seq)),
        running: Arc::new(AtomicBool::new(running)),
        stream_hits: Arc::new(AtomicUsize::new(0)),
        bodies: Arc::new(bodies),
    };
    let app = Router::new()
        .route("/api/v1/sessions/s1/pending", get(pending))
        .route("/api/v1/sessions/s1/stream", get(stream))
        .route("/api/v1/sessions/s1", get(session))
        .with_state(w.clone());
    let srv = spawn_http(app).await;
    let c = PandoClient::new(PandoConfig::new(srv.base_url())).unwrap_or_else(|e| panic!("{e}"));
    (srv, w, c)
}

fn cfg() -> FollowConfig {
    FollowConfig {
        poll_interval: Duration::from_millis(30),
        reattach_delay: Duration::from_millis(10),
    }
}

async fn next(f: &mut pando_client::follow::SessionFollow) -> FollowEvent {
    tokio::time::timeout(Duration::from_secs(5), f.next())
        .await
        .unwrap_or_else(|_| panic!("timeout"))
        .unwrap_or_else(|| panic!("closed"))
}

const RESURRECTED: &str = "event: session\ndata: {\"sessionId\":\"s1\",\"running\":true}\n\nevent: run\ndata: {\"runSeq\":2,\"sessionId\":\"s1\"}\n\nevent: resurrected\ndata: {\"session_id\":\"s1\",\"message\":\"delegated result arrived\"}\n\nevent: content_delta\ndata: {\"text\":\"done!\"}\n\nevent: done\ndata: {}\n\n";

#[tokio::test]
async fn resurrected_run_is_attached_after_run_seq_grows() {
    let (_srv, w, c) = setup(1, false, vec![RESURRECTED]).await;
    let mut f = follow_session(c, "s1", seen_for(false, 1), cfg());
    // Idle: nothing happens while run_seq == seen.
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(w.stream_hits.load(Ordering::SeqCst), 0);

    w.run_seq.store(2, Ordering::SeqCst); // mesnada wakes up on its own
    assert_eq!(next(&mut f).await, FollowEvent::ResetBubble);
    let mut saw_resurrected = false;
    let mut text = String::new();
    loop {
        match next(&mut f).await {
            FollowEvent::Chat(ChatEvent::Resurrected(n)) => {
                assert_eq!(n.message, "delegated result arrived");
                saw_resurrected = true;
            }
            FollowEvent::Chat(ChatEvent::ContentDelta(t)) => text.push_str(&t),
            FollowEvent::RunFinished => break,
            _ => {}
        }
    }
    assert!(saw_resurrected && text == "done!");
    assert_eq!(f.seen(), 2);
    // No second attach for the same run.
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(w.stream_hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn app_restarted_mid_run_replays_then_continues() {
    let (_srv, w, c) = setup(1, true, vec![RESURRECTED]).await;
    // Session opened while running => seen = -1 => attach immediately.
    let mut f = follow_session(c, "s1", seen_for(true, 1), cfg());
    assert_eq!(next(&mut f).await, FollowEvent::ResetBubble);
    assert!(matches!(
        next(&mut f).await,
        FollowEvent::Chat(ChatEvent::Session { .. })
    ));
    assert_eq!(w.stream_hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn stream_closed_without_done_is_reattached() {
    let first = "event: session\ndata: {\"sessionId\":\"s1\",\"running\":true}\n\nevent: run\ndata: {\"runSeq\":1,\"sessionId\":\"s1\"}\n\nevent: content_delta\ndata: {\"text\":\"par\"}\n\n";
    let second = "event: session\ndata: {\"sessionId\":\"s1\",\"running\":true}\n\nevent: content_delta\ndata: {\"text\":\"partial and rest\"}\n\nevent: done\ndata: {}\n\n";
    let (_srv, w, c) = setup(1, true, vec![first, second]).await;
    let mut f = follow_session(c, "s1", -1, cfg());
    let mut resets = 0;
    loop {
        match next(&mut f).await {
            FollowEvent::ResetBubble => resets += 1,
            FollowEvent::RunFinished => break,
            _ => {}
        }
    }
    assert_eq!(resets, 2, "the bubble is reset before each replay");
    assert_eq!(w.stream_hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn empty_replay_reloads_history() {
    let empty = "event: session\ndata: {\"sessionId\":\"s1\",\"running\":false}\n\nevent: goal_status\ndata: {\"goal\":null}\n\nevent: done\ndata: {}\n\n";
    let (_srv, _w, c) = setup(3, false, vec![empty]).await;
    let mut f = follow_session(c, "s1", 0, cfg());
    assert_eq!(next(&mut f).await, FollowEvent::ResetBubble);
    loop {
        match next(&mut f).await {
            FollowEvent::HistoryReload(d) => {
                assert_eq!(d.messages.len(), 1);
                break;
            }
            FollowEvent::RunFinished => panic!("history must be reloaded before RunFinished"),
            _ => {}
        }
    }
}

#[tokio::test]
async fn own_run_is_not_attached_twice() {
    let (_srv, w, c) = setup(1, false, vec![RESURRECTED]).await;
    let f = follow_session(c, "s1", 1, cfg());
    f.begin_local_run();
    w.run_seq.store(2, Ordering::SeqCst); // our own run started
    f.observe(&ChatEvent::Run {
        session_id: "s1".into(),
        run_seq: 2,
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(w.stream_hits.load(Ordering::SeqCst), 0);
    f.observe(&ChatEvent::Done);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        w.stream_hits.load(Ordering::SeqCst),
        0,
        "seen already covers run 2"
    );
    assert_eq!(f.seen(), 2);
}
