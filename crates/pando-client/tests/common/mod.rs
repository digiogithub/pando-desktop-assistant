//! Test helpers: local http/https servers with a throwaway CA.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::service::TowerToHyperService;
use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair, SanType};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls;
use tokio_rustls::rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};

/// A running test server; dropping it stops it.
pub struct TestServer {
    pub addr: SocketAddr,
    pub scheme: &'static str,
    handle: JoinHandle<()>,
}

impl TestServer {
    pub fn base_url(&self) -> String {
        format!("{}://{}", self.scheme, self.addr)
    }
    pub fn stop(&self) {
        self.handle.abort();
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// A throwaway CA that can sign a `localhost`/`127.0.0.1` leaf.
pub struct TestCa {
    pub ca_pem: String,
    issuer_params: CertificateParams,
    issuer_key: KeyPair,
}

impl TestCa {
    pub fn new() -> Self {
        let key = KeyPair::generate().unwrap_or_else(|e| panic!("{e}"));
        let mut params = CertificateParams::new(Vec::new()).unwrap_or_else(|e| panic!("{e}"));
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let cert = params.self_signed(&key).unwrap_or_else(|e| panic!("{e}"));
        Self {
            ca_pem: cert.pem(),
            issuer_params: params,
            issuer_key: key,
        }
    }

    fn server_config(&self) -> Arc<rustls::ServerConfig> {
        let leaf_key = KeyPair::generate().unwrap_or_else(|e| panic!("{e}"));
        let mut params =
            CertificateParams::new(vec!["localhost".to_owned()]).unwrap_or_else(|e| panic!("{e}"));
        params
            .subject_alt_names
            .push(SanType::IpAddress(std::net::Ipv4Addr::LOCALHOST.into()));
        let issuer = Issuer::from_params(&self.issuer_params, &self.issuer_key);
        let leaf = params
            .signed_by(&leaf_key, &issuer)
            .unwrap_or_else(|e| panic!("{e}"));
        let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
        // rcgen enables the `ring` feature next to `aws-lc-rs`, so the process has two
        // candidate providers; name one instead of relying on the crate-feature default.
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let cfg = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_no_client_auth()
            .with_single_cert(vec![leaf.der().clone()], key)
            .unwrap_or_else(|e| panic!("{e}"));
        Arc::new(cfg)
    }
}

pub async fn spawn_http(app: Router) -> TestServer {
    spawn_http_at("127.0.0.1:0".parse().unwrap_or_else(|e| panic!("{e}")), app).await
}

pub async fn spawn_http_at(addr: SocketAddr, app: Router) -> TestServer {
    let listener = TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let addr = listener.local_addr().unwrap_or_else(|e| panic!("{e}"));
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    TestServer {
        addr,
        scheme: "http",
        handle,
    }
}

pub async fn spawn_https(ca: &TestCa, app: Router) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    let addr = listener.local_addr().unwrap_or_else(|e| panic!("{e}"));
    let acceptor = TlsAcceptor::from(ca.server_config());
    let handle = tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            let svc = TowerToHyperService::new(app.clone());
            tokio::spawn(async move {
                if let Ok(tls) = acceptor.accept(tcp).await {
                    let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                        .serve_connection(TokioIo::new(tls), svc)
                        .await;
                }
            });
        }
    });
    TestServer {
        addr,
        scheme: "https",
        handle,
    }
}
