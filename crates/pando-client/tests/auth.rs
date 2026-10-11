//! TLS with a local CA, ephemeral token refresh on 401, Basic auth and secret hygiene.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::get;
use common::{TestCa, spawn_http, spawn_https};
use pando_client::auth::BoxFuture;
use pando_client::{
    AuthHooks, BasicAuth, CredentialPrompt, Error, MemorySecretStore, PandoClient, PandoConfig,
    SecretStore,
};

#[derive(Clone)]
struct Pando {
    token: Arc<Mutex<String>>,
    token_fetches: Arc<AtomicUsize>,
    basic: Option<(&'static str, &'static str)>,
}

impl Pando {
    fn new(token: &str) -> Self {
        Self {
            token: Arc::new(Mutex::new(token.to_owned())),
            token_fetches: Arc::new(AtomicUsize::new(0)),
            basic: None,
        }
    }
    fn rotate(&self, token: &str) {
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) = token.to_owned();
    }
    fn current(&self) -> String {
        self.token.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn app(&self) -> Router {
        Router::new()
            .route("/api/v1/token", get(token_handler))
            .route("/api/v1/ping", get(ping_handler))
            .with_state(self.clone())
    }
}

fn basic_ok(p: &Pando, h: &HeaderMap) -> bool {
    let Some((u, pw)) = p.basic else { return true };
    let expect = {
        // base64("u:pw") computed by reqwest in the client; compare by decoding ourselves
        let raw = format!("{u}:{pw}");
        let t = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut o = String::new();
        for c in raw.as_bytes().chunks(3) {
            let n = (u32::from(c[0]) << 16)
                | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
                | u32::from(*c.get(2).unwrap_or(&0));
            for i in 0..4 {
                if i <= c.len() {
                    o.push(char::from(t[((n >> (18 - 6 * i)) & 63) as usize]));
                } else {
                    o.push('=');
                }
            }
        }
        format!("Basic {o}")
    };
    h.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) == Some(expect.as_str())
}

async fn token_handler(State(p): State<Pando>, h: HeaderMap) -> (StatusCode, String) {
    if !basic_ok(&p, &h) {
        return (
            StatusCode::UNAUTHORIZED,
            r#"{"error":"basic_auth_required"}"#.into(),
        );
    }
    p.token_fetches.fetch_add(1, Ordering::SeqCst);
    (StatusCode::OK, format!(r#"{{"token":"{}"}}"#, p.current()))
}

async fn ping_handler(State(p): State<Pando>, h: HeaderMap) -> (StatusCode, String) {
    if !basic_ok(&p, &h) {
        return (
            StatusCode::UNAUTHORIZED,
            r#"{"error":"basic_auth_required"}"#.into(),
        );
    }
    let ok = h.get("x-pando-token").and_then(|v| v.to_str().ok()) == Some(p.current().as_str());
    if ok {
        (StatusCode::OK, r#"{"ok":true}"#.into())
    } else {
        (
            StatusCode::UNAUTHORIZED,
            r#"{"error":"unauthorized"}"#.into(),
        )
    }
}

#[tokio::test]
async fn https_with_local_ca_works_and_without_it_fails() {
    let ca = TestCa::new();
    let pando = Pando::new("t1");
    let srv = spawn_https(&ca, pando.app()).await;
    let ok = PandoClient::new(
        PandoConfig::new(srv.base_url())
            .with_root_certificate_pem(ca.ca_pem.clone().into_bytes())
            .with_token("t1"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let v: serde_json::Value = ok
        .get_json("/api/v1/ping")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(v["ok"], true);

    let bad = PandoClient::new(PandoConfig::new(srv.base_url()).with_token("t1"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(
        bad.get_json::<serde_json::Value>("/api/v1/ping").await,
        Err(Error::Unreachable(_))
    ));
}

#[tokio::test]
async fn server_restart_with_new_token_is_retried_transparently() {
    let ca = TestCa::new();
    let pando = Pando::new("old-token");
    let srv = spawn_https(&ca, pando.app()).await;
    let client = PandoClient::new(
        PandoConfig::new(srv.base_url())
            .with_root_certificate_pem(ca.ca_pem.clone().into_bytes())
            .with_token("old-token"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let _: serde_json::Value = client
        .get_json("/api/v1/ping")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(pando.token_fetches.load(Ordering::SeqCst), 0);

    pando.rotate("new-token"); // "restart"
    let v: serde_json::Value = client
        .get_json("/api/v1/ping")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(v["ok"], true);
    assert_eq!(pando.token_fetches.load(Ordering::SeqCst), 1);
    assert_eq!(
        client.token().map(|t| t.expose().to_owned()).as_deref(),
        Some("new-token")
    );
}

#[tokio::test]
async fn burst_of_401s_triggers_a_single_token_refetch() {
    let pando = Pando::new("a");
    let srv = spawn_http(pando.app()).await;
    let client = PandoClient::new(PandoConfig::new(srv.base_url()).with_token("a"))
        .unwrap_or_else(|e| panic!("{e}"));
    pando.rotate("b");
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            c.get_json::<serde_json::Value>("/api/v1/ping")
                .await
                .is_ok()
        }));
    }
    for t in tasks {
        assert!(t.await.unwrap_or(false));
    }
    assert_eq!(pando.token_fetches.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn persistent_401_is_surfaced_not_looped() {
    // the server always rejects: one refetch, one retry, then Unauthorized
    let app = Router::new()
        .route("/api/v1/token", get(|| async { r#"{"token":"x"}"# }))
        .route(
            "/api/v1/ping",
            get(|| async { (StatusCode::UNAUTHORIZED, r#"{"error":"unauthorized"}"#) }),
        );
    let srv = spawn_http(app).await;
    let c = PandoClient::new(PandoConfig::new(srv.base_url()).with_token("a"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(
        c.get_json::<serde_json::Value>("/api/v1/ping").await,
        Err(Error::Unauthorized)
    ));
}

struct CountingPrompt {
    calls: AtomicUsize,
    answer: Option<BasicAuth>,
}

impl CredentialPrompt for CountingPrompt {
    fn prompt<'a>(&'a self, _base_url: &'a str) -> BoxFuture<'a, Option<BasicAuth>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            self.answer.clone()
        })
    }
}

#[tokio::test]
async fn basic_auth_prompts_once_for_a_burst_and_remembers_in_the_store() {
    let mut pando = Pando::new("tok");
    pando.basic = Some(("alice", "s3cret"));
    let srv = spawn_http(pando.app()).await;
    let store: Arc<dyn SecretStore> = Arc::new(MemorySecretStore::default());
    let prompt = Arc::new(CountingPrompt {
        calls: AtomicUsize::new(0),
        answer: Some(BasicAuth::new("alice", "s3cret")),
    });
    let hooks = AuthHooks {
        store: Some(store.clone()),
        prompt: Some(prompt.clone()),
    };
    let client = PandoClient::with_hooks(PandoConfig::new(srv.base_url()), hooks)
        .unwrap_or_else(|e| panic!("{e}"));

    let mut tasks = Vec::new();
    for _ in 0..6 {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            c.get_json::<serde_json::Value>("/api/v1/ping")
                .await
                .is_ok()
        }));
    }
    for t in tasks {
        assert!(t.await.unwrap_or(false));
    }
    assert_eq!(prompt.calls.load(Ordering::SeqCst), 1, "single dialog");

    // a fresh client reuses the stored credentials without any dialog
    let prompt2 = Arc::new(CountingPrompt {
        calls: AtomicUsize::new(0),
        answer: None,
    });
    let hooks2 = AuthHooks {
        store: Some(store),
        prompt: Some(prompt2.clone()),
    };
    let client2 = PandoClient::with_hooks(PandoConfig::new(srv.base_url()), hooks2)
        .unwrap_or_else(|e| panic!("{e}"));
    let v: serde_json::Value = client2
        .get_json("/api/v1/ping")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(v["ok"], true);
    assert_eq!(prompt2.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancelled_dialog_reports_basic_auth_required() {
    let mut pando = Pando::new("tok");
    pando.basic = Some(("alice", "s3cret"));
    let srv = spawn_http(pando.app()).await;
    let prompt = Arc::new(CountingPrompt {
        calls: AtomicUsize::new(0),
        answer: None,
    });
    let hooks = AuthHooks {
        store: None,
        prompt: Some(prompt),
    };
    let client = PandoClient::with_hooks(PandoConfig::new(srv.base_url()), hooks)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(
        client.get_json::<serde_json::Value>("/api/v1/ping").await,
        Err(Error::BasicAuthRequired)
    ));
}

#[tokio::test]
async fn wrong_stored_credentials_are_dropped_and_the_user_is_asked() {
    let mut pando = Pando::new("tok");
    pando.basic = Some(("alice", "s3cret"));
    let srv = spawn_http(pando.app()).await;
    let store: Arc<dyn SecretStore> = Arc::new(MemorySecretStore::default());
    let account = format!("basic:{}", srv.base_url());
    let _ = store.set(&account, r#"{"u":"alice","p":"stale"}"#);
    let prompt = Arc::new(CountingPrompt {
        calls: AtomicUsize::new(0),
        answer: Some(BasicAuth::new("alice", "s3cret")),
    });
    let hooks = AuthHooks {
        store: Some(store.clone()),
        prompt: Some(prompt.clone()),
    };
    let client = PandoClient::with_hooks(PandoConfig::new(srv.base_url()), hooks)
        .unwrap_or_else(|e| panic!("{e}"));
    let v: serde_json::Value = client
        .get_json("/api/v1/ping")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(v["ok"], true);
    assert_eq!(prompt.calls.load(Ordering::SeqCst), 1);
    assert!(
        store
            .get(&account)
            .ok()
            .flatten()
            .is_some_and(|s| s.contains("s3cret"))
    );
}
