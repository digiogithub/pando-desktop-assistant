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
