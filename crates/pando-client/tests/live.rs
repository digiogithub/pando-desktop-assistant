//! Live tests against a real local Pando (`pando serve`). Run with
//! `cargo test -p pando-client --test live -- --ignored`.

use pando_client::auth::{default_ca_path, load_local_ca};
use pando_client::{Discovery, PandoClient, PandoConfig};

#[tokio::test]
#[ignore = "needs a running `pando serve`"]
async fn discovers_and_authenticates_against_a_local_pando() {
    let ca = default_ca_path().and_then(|p| load_local_ca(&p).ok().flatten());
    let disc = Discovery {
        root_certificate_pem: ca.clone(),
        ..Discovery::default()
    };
    let found = disc.discover().await.unwrap_or_else(|e| panic!("{e}"));
    let mut cfg = PandoConfig::new(found.base_url);
    if let Some(pem) = ca {
        cfg = cfg.with_root_certificate_pem(pem);
    }
    let client = PandoClient::new(cfg).unwrap_or_else(|e| panic!("{e}"));
    // No token yet: the first 401 triggers the loopback token fetch.
    let _: serde_json::Value = client
        .get_json("/api/v1/models")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(client.token().is_some());
}

#[tokio::test]
#[ignore = "needs a running `pando serve`"]
async fn connection_service_connects_to_a_local_pando() {
    use pando_client::{ConnectionOptions, ConnectionService, ConnectionState};
    let ca = default_ca_path().and_then(|p| load_local_ca(&p).ok().flatten());
    let svc = ConnectionService::start(ConnectionOptions {
        discovery: Discovery {
            root_certificate_pem: ca,
            ..Discovery::default()
        },
        ..ConnectionOptions::default()
    });
    let _link = tokio::time::timeout(std::time::Duration::from_secs(10), svc.link())
        .await
        .unwrap_or_else(|_| panic!("not connected: {:?}", svc.state()));
    assert!(matches!(svc.state(), ConnectionState::Connected { .. }));
}

#[tokio::test]
#[ignore = "needs a running `pando serve` and spends a few LLM tokens"]
async fn live_chat_then_reattach_replays_the_run() {
    use pando_client::follow::{FollowConfig, FollowEvent, follow_session};
    use pando_client::{ChatEvent, ConnectionOptions, ConnectionService};

    let ca = default_ca_path().and_then(|p| load_local_ca(&p).ok().flatten());
    let svc = ConnectionService::start(ConnectionOptions {
        discovery: Discovery {
            root_certificate_pem: ca,
            ..Discovery::default()
        },
        ..ConnectionOptions::default()
    });
    let link = tokio::time::timeout(std::time::Duration::from_secs(10), svc.link())
        .await
        .unwrap_or_else(|_| panic!("not connected"));
    let client = link.client.clone();

    let mut chat = client
        .chat_stream(
            None,
            "Reply with exactly the word: pong. Do not use tools.",
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let mut sid = String::new();
    let mut text = String::new();
    while let Some(ev) = chat.next().await.unwrap_or_else(|e| panic!("{e}")) {
        match ev {
            ChatEvent::Session { session_id, .. } if sid.is_empty() => sid = session_id,
            ChatEvent::ContentDelta(t) => text.push_str(&t),
            ChatEvent::Done => break,
            _ => {}
        }
    }
    assert!(text.to_lowercase().contains("pong"), "{text:?}");

    // run_seq protocol against the real server: a follower that has not seen the run attaches,
    // gets the buffered replay (reset first) and finishes.
    let mut f = follow_session(
        client.clone(),
        &sid,
        0,
        FollowConfig {
            poll_interval: std::time::Duration::from_millis(200),
            ..FollowConfig::default()
        },
    );
    let mut replayed = String::new();
    let mut reset = false;
    loop {
        let ev = tokio::time::timeout(std::time::Duration::from_secs(10), f.next())
            .await
            .unwrap_or_else(|_| panic!("follow timeout"))
            .unwrap_or_else(|| panic!("follow ended"));
        match ev {
            FollowEvent::ResetBubble => reset = true,
            FollowEvent::Chat(ChatEvent::ContentDelta(t)) => replayed.push_str(&t),
            FollowEvent::RunFinished => break,
            _ => {}
        }
    }
    assert!(reset);
    assert_eq!(replayed, text);
    let _ = client.delete_session(&sid).await;
}
