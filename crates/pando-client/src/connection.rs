//! `ConnectionService`: keeps one healthy [`PandoClient`] and tells everyone when it changes.
//!
//! States: `Connecting | Connected{version, project} | Unavailable{reason} | Unauthorized |
//! TooOld{min}`. While connected it probes `/health` every `health_interval` (30 s by default);
//! while not connected it retries with exponential backoff (1 s doubling to 30 s). Every
//! transition into `Connected` bumps a *generation*; long-lived streams hold a [`Link`] and
//! reopen when [`Link::closed`] resolves, which gives the coordinated close/reopen.
//!
//! Needs a tokio runtime (it spawns one task). The UI observes with [`ConnectionService::subscribe`].

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;

use crate::auth::AuthHooks;
use crate::client::PandoClient;
use crate::config::{PandoConfig, Token};
use crate::discovery::Discovery;
use crate::error::Error;

/// Connection state, as shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// Looking for / contacting Pando.
    Connecting,
    /// Healthy and authenticated.
    Connected {
        /// Server version, e.g. `v1.3.0`.
        version: String,
        /// Active project name, `None` when the server has none.
        project: Option<String>,
    },
    /// Not reachable (or answering nonsense).
    Unavailable {
        /// Short human-readable cause (never contains secrets).
        reason: String,
    },
    /// Reachable but credentials are missing or rejected.
    Unauthorized,
    /// Reachable but older than the minimum supported version.
    TooOld {
        /// Minimum supported version.
        min: String,
        /// Version the server reported.
        found: String,
    },
}

/// What watchers see: the state plus the client to use while connected.
#[derive(Clone)]
pub struct Snapshot {
    /// Current state.
    pub state: ConnectionState,
    /// Bumped on every entry into `Connected`; `0` before the first connection.
    pub generation: u64,
    /// The client, present only while `Connected`.
    pub client: Option<PandoClient>,
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snapshot")
            .field("state", &self.state)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

/// Lenient `major.minor.patch` (a leading `v` and any `-pre`/`+meta` suffix are ignored).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);

impl Version {
    /// Parses `v1.3.0`, `1.3`, `1.3.0-rc1`. `None` for non-numeric strings such as `dev`.
    pub fn parse(raw: &str) -> Option<Self> {
        let s = raw.trim().trim_start_matches('v');
        let core = s.split(['-', '+']).next()?;
        let mut it = core.split('.');
        let major = it.next()?.parse().ok()?;
        let minor = it.next().map_or(Some(0), |p| p.parse().ok())?;
        let patch = it.next().map_or(Some(0), |p| p.parse().ok())?;
        Some(Self(major, minor, patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Tuning and wiring of the service.
#[derive(Debug, Clone)]
pub struct ConnectionOptions {
    /// How to find the server.
    pub discovery: Discovery,
    /// Initial token (normally `None`: it is fetched on the first 401).
    pub token: Option<Token>,
    /// Keychain / login dialog hooks for Basic auth.
    pub hooks: AuthHooks,
    /// Oldest supported Pando.
    pub min_version: Version,
    /// Probe period while connected.
    pub health_interval: Duration,
    /// First retry delay while disconnected.
    pub backoff_min: Duration,
    /// Retry delay cap.
    pub backoff_max: Duration,
    /// Authenticated GET used to validate credentials on connect.
    pub auth_check_path: String,
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            discovery: Discovery::default(),
            token: None,
            hooks: AuthHooks::default(),
            min_version: Version(1, 0, 0),
            health_interval: Duration::from_secs(30),
            backoff_min: Duration::from_secs(1),
            backoff_max: Duration::from_secs(30),
            auth_check_path: "/api/v1/sessions?limit=1".to_owned(),
        }
    }
}

/// Handle to the background connection task. Dropping it stops the task.
#[derive(Debug)]
pub struct ConnectionService {
    rx: watch::Receiver<Snapshot>,
    nudge: Arc<Notify>,
    task: JoinHandle<()>,
}

impl ConnectionService {
    /// Starts the service on the current tokio runtime.
    pub fn start(options: ConnectionOptions) -> Self {
        let (tx, rx) = watch::channel(Snapshot {
            state: ConnectionState::Connecting,
            generation: 0,
            client: None,
        });
        let nudge = Arc::new(Notify::new());
        let task = tokio::spawn(run(options, tx, nudge.clone()));
        Self { rx, nudge, task }
    }

    /// The current state.
    pub fn state(&self) -> ConnectionState {
        self.rx.borrow().state.clone()
    }

    /// A receiver that wakes on every state change (UI events).
    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.rx.clone()
    }

    /// Asks for an immediate re-probe (call it when a stream or request failed in transport).
    pub fn nudge(&self) {
        self.nudge.notify_one();
    }

    /// Waits until connected and returns a [`Link`] for the current generation.
    pub async fn link(&self) -> Link {
        let mut rx = self.rx.clone();
        loop {
            {
                let snap = rx.borrow_and_update();
                if let (ConnectionState::Connected { .. }, Some(client)) =
                    (&snap.state, &snap.client)
                {
                    return Link {
                        client: client.clone(),
                        generation: snap.generation,
                        rx: self.rx.clone(),
                    };
                }
            }
            if rx.changed().await.is_err() {
                // The task ended; park forever rather than spin (service dropped elsewhere).
                std::future::pending::<()>().await;
            }
        }
    }

    /// Stops the background task.
    pub fn stop(&self) {
        self.task.abort();
    }
}

impl Drop for ConnectionService {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A client bound to one connection generation. Streams `select!` on [`Link::closed`], then call
/// [`ConnectionService::link`] again to reopen against the new connection.
#[derive(Debug, Clone)]
pub struct Link {
    /// The client of this generation.
    pub client: PandoClient,
    generation: u64,
    rx: watch::Receiver<Snapshot>,
}

impl Link {
    /// The generation this link belongs to.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Resolves once this generation is over (disconnected, or reconnected as a new generation).
    pub async fn closed(&mut self) {
        loop {
            {
                let snap = self.rx.borrow_and_update();
                if snap.generation != self.generation
                    || !matches!(snap.state, ConnectionState::Connected { .. })
                {
                    return;
                }
            }
            if self.rx.changed().await.is_err() {
                return;
            }
        }
    }
}

/// Outcome of one connection attempt.
struct Connected {
    client: PandoClient,
    state: ConnectionState,
    base_url: String,
}

async fn connect(options: &ConnectionOptions) -> std::result::Result<Connected, ConnectionState> {
    let found = options
        .discovery
        .discover()
        .await
        .map_err(|e| unavailable(&e))?;
    if let Some(v) = Version::parse(&found.info.version)
        && v < options.min_version
    {
        return Err(ConnectionState::TooOld {
            min: options.min_version.to_string(),
            found: found.info.version,
        });
    }
    let mut cfg = PandoConfig::new(found.base_url.clone());
    cfg.root_certificate_pem = options.discovery.root_certificate_pem.clone();
    cfg.token = options.token.clone();
    let client =
        PandoClient::with_hooks(cfg, options.hooks.clone()).map_err(|e| unavailable(&e))?;
    // Validates credentials (token refetch / Basic recovery happen inside `send`).
    client
        .get_json::<serde_json::Value>(&options.auth_check_path)
        .await
        .map_err(|e| match e {
            Error::Unauthorized | Error::BasicAuthRequired => ConnectionState::Unauthorized,
            other => unavailable(&other),
        })?;
    Ok(Connected {
        client,
        base_url: found.base_url,
        state: connected_state(&found.info),
    })
}

fn connected_state(info: &crate::client::ServerInfo) -> ConnectionState {
    ConnectionState::Connected {
        version: info.version.clone(),
        project: (!info.project_name.is_empty()).then(|| info.project_name.clone()),
    }
}

fn unavailable(e: &Error) -> ConnectionState {
    ConnectionState::Unavailable {
        reason: e.to_string(),
    }
}

async fn run(options: ConnectionOptions, tx: watch::Sender<Snapshot>, nudge: Arc<Notify>) {
    let mut generation = 0u64;
    let mut backoff = options.backoff_min;
    loop {
        match connect(&options).await {
            Ok(conn) => {
                backoff = options.backoff_min;
                generation += 1;
                tracing::debug!(base = %conn.base_url, generation, "pando connected");
                let _ = tx.send(Snapshot {
                    state: conn.state,
                    generation,
                    client: Some(conn.client.clone()),
                });
                // Connected: probe until it fails.
                let failure = loop {
                    tokio::select! {
                        () = tokio::time::sleep(options.health_interval) => {}
                        () = nudge.notified() => {}
                    }
                    match probe(&options, &conn.client).await {
                        Ok(state) => {
                            tx.send_if_modified(|s| {
                                if s.state == state {
                                    false
                                } else {
                                    s.state = state;
                                    true
                                }
                            });
                        }
                        Err(state) => break state,
                    }
                };
                tracing::debug!(?failure, "pando connection lost");
                let _ = tx.send(Snapshot {
                    state: failure,
                    generation,
                    client: None,
                });
                // First retry right away: the instance may just have restarted.
            }
            Err(state) => {
                tx.send_if_modified(|s| {
                    if s.state == state && s.client.is_none() {
                        false
                    } else {
                        s.state = state;
                        s.client = None;
                        true
                    }
                });
                tokio::select! {
                    () = tokio::time::sleep(backoff) => {}
                    () = nudge.notified() => { backoff = options.backoff_min; }
                }
                backoff = (backoff * 2).min(options.backoff_max);
            }
        }
    }
}

/// One health probe of an established connection.
async fn probe(
    options: &ConnectionOptions,
    client: &PandoClient,
) -> std::result::Result<ConnectionState, ConnectionState> {
    let info = client.info().await.map_err(|e| unavailable(&e))?;
    if info.status != "healthy" {
        return Err(ConnectionState::Unavailable {
            reason: format!("health status {:?}", info.status),
        });
    }
    if let Some(v) = Version::parse(&info.version)
        && v < options.min_version
    {
        return Err(ConnectionState::TooOld {
            min: options.min_version.to_string(),
            found: info.version,
        });
    }
    Ok(connected_state(&info))
}

#[cfg(test)]
mod tests {
    use super::Version;

    #[test]
    fn version_parsing() {
        assert_eq!(Version::parse("v1.3.0"), Some(Version(1, 3, 0)));
        assert_eq!(Version::parse("1.3"), Some(Version(1, 3, 0)));
        assert_eq!(Version::parse("2.0.1-rc1+abc"), Some(Version(2, 0, 1)));
        assert_eq!(Version::parse("dev"), None);
        assert!(Version(1, 10, 0) > Version(1, 9, 9));
    }
}
