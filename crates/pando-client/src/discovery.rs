//! Finding a running Pando: the instance registry, the URL policy and the `/health` probe.
//!
//! Pando writes one JSON file per live process into `/tmp/pando-instances/` (hard-coded in
//! Pando; see `docs/research/pando-api.md` section 1.2). Entries carry the web port but neither
//! scheme nor token, so each candidate is probed with `GET /health` (unauthenticated), https
//! first for `webui`/`serve` and http first for `desktop`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::Url;
use serde::Deserialize;

use crate::client::{PandoClient, ServerInfo};
use crate::config::PandoConfig;
use crate::error::{Error, Result};

/// Directory where Pando registers its instances.
pub const DEFAULT_REGISTRY_DIR: &str = "/tmp/pando-instances";
/// Port tried when the registry yields nothing.
pub const FALLBACK_PORT: u16 = 8765;
/// Probe timeout per candidate.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// One `<instance_id>.json` file of the registry (unknown fields are ignored).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RegistryEntry {
    /// Instance UUID.
    pub instance_id: String,
    /// Working directory of the process.
    pub path: String,
    /// Process id.
    pub pid: u32,
    /// HTTP API port, `0` when the process serves no API (TUI, ACP).
    pub web_port: u16,
    /// `tui|webui|desktop|acp|noninteractive|proxy`.
    pub mode: String,
    /// Parent instance of a project child, empty otherwise.
    pub parent_instance_id: String,
    /// The primary instance of its project.
    pub is_primary: bool,
}

/// Reads every parsable `*.json` file in `dir`. Missing directory or corrupt files yield nothing
/// rather than an error: discovery must degrade to the fallback port.
pub fn read_registry(dir: &Path) -> Vec<RegistryEntry> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for ent in rd.flatten() {
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if let Ok(entry) = serde_json::from_slice::<RegistryEntry>(&bytes) {
            out.push(entry);
        }
    }
    out
}

/// Whether `host` only accepts local connections.
fn is_local_host(host: &str) -> bool {
    let host = host.trim_matches(|c| c == '[' || c == ']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// Validates a user-supplied or discovered base URL.
///
/// Rules: scheme must be http or https; no embedded credentials; a non-local host is accepted
/// only over https and only when `allow_remote` is set.
pub fn validate_url(raw: &str, allow_remote: bool) -> Result<Url> {
    let url = Url::parse(raw.trim()).map_err(|e| Error::Config(format!("invalid url: {e}")))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(Error::Config("url scheme must be http or https".to_owned()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::Config("url must not embed credentials".to_owned()));
    }
    let host = url
        .host_str()
        .ok_or_else(|| Error::Config("url has no host".to_owned()))?;
    if !is_local_host(host) {
        if !allow_remote {
            return Err(Error::Config(
                "remote servers are disabled (allow_remote is off)".to_owned(),
            ));
        }
        if url.scheme() != "https" {
            return Err(Error::Config("remote servers require https".to_owned()));
        }
    }
    Ok(url)
}

/// A server that answered `/health`.
#[derive(Debug, Clone)]
pub struct DiscoveredServer {
    /// Base URL that worked (no trailing slash).
    pub base_url: String,
    /// What `/health` reported.
    pub info: ServerInfo,
    /// The registry entry it came from, `None` for a manual URL or the fallback port.
    pub entry: Option<RegistryEntry>,
}

/// Discovery settings.
#[derive(Debug, Clone)]
pub struct Discovery {
    /// Registry directory.
    pub registry_dir: PathBuf,
    /// Host used with registry ports.
    pub host: String,
    /// Port of the last-resort candidate.
    pub fallback_port: u16,
    /// Skip discovery and probe only this URL.
    pub manual_url: Option<String>,
    /// Allow a manual remote https URL.
    pub allow_remote: bool,
    /// Local CA (PEM) trusted for https probes.
    pub root_certificate_pem: Option<Vec<u8>>,
}

impl Default for Discovery {
    fn default() -> Self {
        Self {
            registry_dir: PathBuf::from(DEFAULT_REGISTRY_DIR),
            host: "localhost".to_owned(),
            fallback_port: FALLBACK_PORT,
            manual_url: None,
            allow_remote: false,
            root_certificate_pem: None,
        }
    }
}

/// A URL to probe, with the registry entry that proposed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Base URL.
    pub base_url: String,
    /// Source entry, if any.
    pub entry: Option<RegistryEntry>,
}

impl Discovery {
    /// Ordered probe list: registry entries with `web_port > 0` (primary, then `desktop`/`webui`,
    /// project children last), https/http order depending on mode, then the fallback port.
    pub fn candidates(&self) -> Vec<Candidate> {
        if let Some(url) = &self.manual_url {
            return vec![Candidate {
                base_url: url.trim().trim_end_matches('/').to_owned(),
                entry: None,
            }];
        }
        let mut entries: Vec<RegistryEntry> = read_registry(&self.registry_dir)
            .into_iter()
            .filter(|e| e.web_port > 0)
            .collect();
        let rank = |e: &RegistryEntry| {
            (
                !e.parent_instance_id.is_empty(),
                !e.is_primary,
                !matches!(e.mode.as_str(), "desktop" | "webui"),
                e.web_port,
            )
        };
        entries.sort_by_key(rank);
        let mut out = Vec::new();
        for e in entries {
            let schemes: [&str; 2] = if e.mode == "desktop" {
                ["http", "https"]
            } else {
                ["https", "http"]
            };
            for scheme in schemes {
                out.push(Candidate {
                    base_url: format!("{scheme}://{}:{}", self.host, e.web_port),
                    entry: Some(e.clone()),
                });
            }
        }
        for scheme in ["https", "http"] {
            let base_url = format!("{scheme}://{}:{}", self.host, self.fallback_port);
            if !out.iter().any(|c| c.base_url == base_url) {
                out.push(Candidate {
                    base_url,
                    entry: None,
                });
            }
        }
        out
    }

    /// Probes the candidates in order and returns the first healthy server.
    pub async fn discover(&self) -> Result<DiscoveredServer> {
        if let Some(url) = &self.manual_url {
            validate_url(url, self.allow_remote)?;
        }
        let mut last = Error::Unreachable("no pando instance found".to_owned());
        for cand in self.candidates() {
            match self.probe(&cand.base_url).await {
                Ok(info) => {
                    return Ok(DiscoveredServer {
                        base_url: cand.base_url,
                        info,
                        entry: cand.entry,
                    });
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// `GET /health` against `base_url`; the answer must say `healthy`.
    pub async fn probe(&self, base_url: &str) -> Result<ServerInfo> {
        let mut cfg = PandoConfig::new(base_url)
            .with_timeout(PROBE_TIMEOUT)
            .with_connect_timeout(PROBE_TIMEOUT);
        if let Some(pem) = &self.root_certificate_pem {
            cfg = cfg.with_root_certificate_pem(pem.clone());
        }
        let info = PandoClient::new(cfg)?.info().await?;
        if info.status != "healthy" {
            return Err(Error::Protocol(format!(
                "unexpected health status {:?}",
                info.status
            )));
        }
        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_policy() {
        assert!(validate_url("http://localhost:8765", false).is_ok());
        assert!(validate_url("https://127.0.0.1:1", false).is_ok());
        assert!(validate_url("http://[::1]:1", false).is_ok());
        assert!(validate_url("ftp://localhost", false).is_err());
        assert!(validate_url("https://u:p@localhost", false).is_err());
        assert!(validate_url("https://pando.example.com", false).is_err());
        assert!(validate_url("http://pando.example.com", true).is_err());
        assert!(validate_url("https://pando.example.com", true).is_ok());
    }

    #[test]
    fn registry_ordering_and_filtering() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let w = |name: &str, body: &str| {
            std::fs::write(dir.path().join(name), body).unwrap_or_else(|e| panic!("{e}"))
        };
        w(
            "a.json",
            r#"{"instance_id":"a","pid":1,"web_port":0,"mode":"tui","is_primary":true}"#,
        );
        w(
            "b.json",
            r#"{"instance_id":"b","pid":2,"web_port":9001,"mode":"webui","is_primary":false}"#,
        );
        w(
            "c.json",
            r#"{"instance_id":"c","pid":3,"web_port":9002,"mode":"desktop","is_primary":true}"#,
        );
        w(
            "d.json",
            r#"{"instance_id":"d","pid":4,"web_port":9003,"mode":"webui","is_primary":true,"parent_instance_id":"x"}"#,
        );
        w("bad.json", "{not json");
        w("note.txt", "x");
        let d = Discovery {
            registry_dir: dir.path().to_owned(),
            ..Discovery::default()
        };
        let urls: Vec<String> = d.candidates().into_iter().map(|c| c.base_url).collect();
        assert_eq!(
            urls,
            vec![
                "http://localhost:9002",
                "https://localhost:9002",
                "https://localhost:9001",
                "http://localhost:9001",
                "https://localhost:9003",
                "http://localhost:9003",
                "https://localhost:8765",
                "http://localhost:8765",
            ]
        );
    }

    #[test]
    fn missing_registry_falls_back_to_default_port() {
        let d = Discovery {
            registry_dir: PathBuf::from("/nonexistent/pando-instances"),
            ..Discovery::default()
        };
        assert_eq!(d.candidates().len(), 2);
    }
}
