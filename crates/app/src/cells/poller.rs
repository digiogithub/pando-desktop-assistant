//! Polls Pando for tasks, instances and pending items and sends [`Poll`]s to the UI
//! (PDA-T-0055). There is no orchestrator push stream (gap G1), so this runs every 1 s while
//! the window is visible and every 5 s while hidden; it is meant to be replaced by that
//! stream. It follows the connection: no client, no polling.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pando_client::api::{InstanceInfo, TaskResponse};
use pando_client::chat::PermissionRequest;
use pando_client::connection::Snapshot;
use pando_client::{PandoClient, Result as ClientResult};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::source::Poll;

/// Poll period while the window is visible.
pub const VISIBLE_INTERVAL: Duration = Duration::from_secs(1);
/// Poll period while the window is hidden.
pub const HIDDEN_INTERVAL: Duration = Duration::from_secs(5);

/// Messages for the UI.
#[derive(Clone, Debug)]
pub enum PollMsg {
    /// A fresh poll.
    Poll(Poll),
    /// The connection went away: forget the cells.
    Disconnected,
}

/// Everything one cycle fetches.
#[derive(Clone, Debug, Default)]
pub struct Fetched {
    /// Tasks.
    pub tasks: Vec<TaskResponse>,
    /// Instances.
    pub instances: Vec<InstanceInfo>,
    /// The primary session has pending permissions or questions.
    pub primary_pending: bool,
    /// Pending permission requests of the primary session and of the sessions of ACP tasks
    /// that wait for a permission (the approvals ledger registers them).
    pub permissions: Vec<PermissionRequest>,
}

/// Fetches the data of one cycle from the real client.
pub async fn fetch_all(client: &PandoClient, primary: Option<&str>) -> ClientResult<Fetched> {
    let tasks = client.list_tasks().await?.tasks;
    // Instances and pending are best-effort: an older server without them still shows tasks.
    let instances = client.list_instances().await.unwrap_or_default();
    let mut primary_pending = false;
    let mut permissions = Vec::new();
    if let Some(id) = primary
        && let Ok(p) = client.pending(id).await
    {
        primary_pending = !p.permissions.is_empty() || !p.questions.is_empty();
        permissions.extend(p.permissions);
    }
    for t in tasks
        .iter()
        .filter(|t| super::mapping::waits_for_permission(t))
    {
        let Some(sid) = t.acp_session_id.as_deref().filter(|s| !s.is_empty()) else {
            continue;
        };
        if Some(sid) == primary {
            continue;
        }
        if let Ok(p) = client.pending(sid).await {
            permissions.extend(p.permissions);
        }
    }
    Ok(Fetched {
        tasks,
        instances,
        primary_pending,
        permissions,
    })
}

fn epoch_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// Controls of a running poller; dropping it stops the task.
#[derive(Debug)]
pub struct Poller {
    task: JoinHandle<()>,
    visible: Arc<AtomicBool>,
    primary: watch::Sender<Option<String>>,
}

impl Poller {
    /// Starts polling with the real client of `snapshot`.
    pub fn start(
        handle: &Handle,
        snapshot: watch::Receiver<Snapshot>,
        tx: async_channel::Sender<PollMsg>,
    ) -> Self {
        Self::start_with(
            handle,
            snapshot,
            tx,
            (VISIBLE_INTERVAL, HIDDEN_INTERVAL),
            |client: PandoClient, primary: Option<String>| async move {
                fetch_all(&client, primary.as_deref()).await
            },
        )
    }

    /// Like [`Poller::start`] with an injectable fetch and intervals (tests).
    pub fn start_with<F, Fut>(
        handle: &Handle,
        mut snapshot: watch::Receiver<Snapshot>,
        tx: async_channel::Sender<PollMsg>,
        intervals: (Duration, Duration),
        fetch: F,
    ) -> Self
    where
        F: Fn(PandoClient, Option<String>) -> Fut + Send + 'static,
        Fut: Future<Output = ClientResult<Fetched>> + Send + 'static,
    {
        let visible = Arc::new(AtomicBool::new(true));
        let (primary, primary_rx) = watch::channel(None::<String>);
        let vis = visible.clone();
        let task = handle.spawn(async move {
            loop {
                let (client, generation) = {
                    let snap = snapshot.borrow_and_update();
                    (snap.client.clone(), snap.generation)
                };
                let Some(client) = client else {
                    if tx.send(PollMsg::Disconnected).await.is_err() {
                        return;
                    }
                    if snapshot.changed().await.is_err() {
                        return;
                    }
                    continue;
                };
                loop {
                    let primary_id = primary_rx.borrow().clone();
                    match fetch(client.clone(), primary_id).await {
                        Ok(f) => {
                            let poll = Poll {
                                tasks: f.tasks,
                                instances: f.instances,
                                primary_pending: f.primary_pending,
                                permissions: f.permissions,
                                now: epoch_now(),
                            };
                            if tx.send(PollMsg::Poll(poll)).await.is_err() {
                                return;
                            }
                        }
                        Err(err) => tracing::debug!("cell poll failed: {err}"),
                    }
                    let wait = if vis.load(Ordering::Relaxed) {
                        intervals.0
                    } else {
                        intervals.1
                    };
                    tokio::select! {
                        () = tokio::time::sleep(wait) => {}
                        changed = snapshot.changed() => {
                            if changed.is_err() {
                                return;
                            }
                            if snapshot.borrow().generation != generation
                                || snapshot.borrow().client.is_none()
                            {
                                break;
                            }
                        }
                    }
                }
            }
        });
        Self {
            task,
            visible,
            primary,
        }
    }

    /// Visible windows poll every second, hidden ones every five.
    pub fn set_visible(&self, visible: bool) {
        self.visible.store(visible, Ordering::Relaxed);
    }

    /// The primary chat session whose pending items make the primary cell wait.
    pub fn set_primary_session(&self, id: Option<String>) {
        self.primary.send_replace(id);
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pando_client::{ConnectionState, PandoConfig};
    use std::sync::atomic::AtomicUsize;

    fn connected(generation: u64) -> Snapshot {
        Snapshot {
            state: ConnectionState::Connected {
                version: "v1".into(),
                project: None,
            },
            generation,
            client: Some(PandoClient::new(PandoConfig::new("http://127.0.0.1:9")).expect("client")),
        }
    }

    fn disconnected() -> Snapshot {
        Snapshot {
            state: ConnectionState::Connecting,
            generation: 0,
            client: None,
        }
    }

    #[allow(clippy::disallowed_methods)]
    #[tokio::test]
    async fn polls_while_connected_and_reports_disconnection() {
        let (stx, srx) = watch::channel(disconnected());
        let (tx, rx) = async_channel::unbounded();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let poller = Poller::start_with(
            &Handle::current(),
            srx,
            tx,
            (Duration::from_millis(5), Duration::from_millis(50)),
            move |_client, primary| {
                let n = c.fetch_add(1, Ordering::SeqCst);
                async move {
                    Ok(Fetched {
                        tasks: vec![TaskResponse {
                            id: format!("t{n}"),
                            ..TaskResponse::default()
                        }],
                        instances: Vec::new(),
                        primary_pending: primary.as_deref() == Some("s1"),
                        permissions: Vec::new(),
                    })
                }
            },
        );
        assert!(matches!(rx.recv().await, Ok(PollMsg::Disconnected)));
        stx.send_replace(connected(1));
        let first = loop {
            if let Ok(PollMsg::Poll(p)) = rx.recv().await {
                break p;
            }
        };
        assert!(!first.primary_pending && first.now > 1.0e9);
        poller.set_primary_session(Some("s1".into()));
        let pending = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(PollMsg::Poll(p)) = rx.recv().await
                    && p.primary_pending
                {
                    return p;
                }
            }
        })
        .await
        .expect("pending poll");
        assert!(pending.primary_pending);
        stx.send_replace(disconnected());
        let gone = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(PollMsg::Disconnected) = rx.recv().await {
                    return;
                }
            }
        })
        .await;
        assert!(gone.is_ok(), "disconnection is reported");
        drop(poller);
        assert!(calls.load(Ordering::SeqCst) >= 2);
    }

    #[allow(clippy::disallowed_methods)]
    #[tokio::test]
    async fn hidden_windows_poll_slower() {
        let (_stx, srx) = watch::channel(connected(1));
        let (tx, rx) = async_channel::unbounded();
        let poller = Poller::start_with(
            &Handle::current(),
            srx,
            tx,
            (Duration::from_millis(5), Duration::from_millis(400)),
            |_c, _p| async { Ok(Fetched::default()) },
        );
        poller.set_visible(false);
        tokio::time::sleep(Duration::from_millis(250)).await;
        let n = rx.len();
        assert!(n <= 2, "hidden poller should be slow, got {n} polls");
    }
}
