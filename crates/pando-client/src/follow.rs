//! The `run_seq` protocol (PDA-SP-0002.R4): keeps a view of one session in sync with runs the
//! client did not start (resurrected runs, another surface, app restarted mid-run).
//!
//! Per followed session:
//! 1. `seen = is_running ? -1 : run_seq` from `GET /sessions/{id}` ([`seen_for`]).
//! 2. Poll `GET /sessions/{id}/pending` (default every 4 s).
//! 3. No stream open and `run_seq > seen`: claim it (`seen = run_seq`), emit
//!    [`FollowEvent::ResetBubble`] (a replay re-delivers everything since the run start, deltas
//!    carry no ids), then open `GET /sessions/{id}/stream`.
//! 4. A replay without any content is followed by [`FollowEvent::HistoryReload`].
//! 5. A stream that closes without `done` is reattached while the run is still active.
//!
//! Gap G5 (a global session-activity stream) does not exist yet; when it does, it should replace
//! step 2 for the sessions it covers. The polling path stays as the fallback.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::chat::{ChatEvent, SessionDetail};
use crate::client::PandoClient;
use crate::error::Error;

/// Initial `seenRunSeq` for a freshly loaded session.
pub fn seen_for(is_running: bool, run_seq: i64) -> i64 {
    if is_running { -1 } else { run_seq }
}

/// Whether the poller must attach now.
pub fn should_attach(seen: i64, stream_open: bool, local_run: bool, pending_run_seq: i64) -> bool {
    !stream_open && !local_run && pending_run_seq > seen
}

/// Tuning.
#[derive(Debug, Clone)]
pub struct FollowConfig {
    /// `/pending` polling period.
    pub poll_interval: Duration,
    /// Pause before reattaching a stream that closed without `done`.
    pub reattach_delay: Duration,
}

impl Default for FollowConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(4),
            reattach_delay: Duration::from_millis(300),
        }
    }
}

/// What the follower tells the UI.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum FollowEvent {
    /// Discard the live (partial) assistant bubble: a full replay follows.
    ResetBubble,
    /// An event of the attached run.
    Chat(ChatEvent),
    /// The replay produced no content: re-render from this history.
    HistoryReload(Box<SessionDetail>),
    /// The attached run reported `done`.
    RunFinished,
}

/// Handle of a background follower. Dropping it stops following.
#[derive(Debug)]
pub struct SessionFollow {
    rx: mpsc::Receiver<FollowEvent>,
    seen: Arc<AtomicI64>,
    local_run: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

impl SessionFollow {
    /// The next event; `None` after the task ended.
    pub async fn next(&mut self) -> Option<FollowEvent> {
        self.rx.recv().await
    }

    /// Current `seenRunSeq`.
    pub fn seen(&self) -> i64 {
        self.seen.load(Ordering::SeqCst)
    }

    /// Call before the app starts its own run on this session, so the poller does not attach to
    /// it as well. Pair with [`SessionFollow::observe`] (which clears it on `Done`).
    pub fn begin_local_run(&self) {
        self.local_run.store(true, Ordering::SeqCst);
    }

    /// Feeds an event of the app's own chat stream: `Run` advances `seenRunSeq`, `Done` (or an
    /// error) ends the local run.
    pub fn observe(&self, ev: &ChatEvent) {
        match ev {
            ChatEvent::Run { run_seq, .. } => {
                self.seen.fetch_max(*run_seq, Ordering::SeqCst);
            }
            ChatEvent::Done | ChatEvent::Error(_) => self.local_run.store(false, Ordering::SeqCst),
            _ => {}
        }
    }
}

impl Drop for SessionFollow {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn has_content(ev: &ChatEvent) -> bool {
    matches!(
        ev,
        ChatEvent::ContentDelta(_)
            | ChatEvent::ThinkingDelta(_)
            | ChatEvent::ToolCall(_)
            | ChatEvent::ToolResult(_)
            | ChatEvent::PlanUpdate(_)
    )
}

/// Starts following `session_id`. `initial_seen` comes from [`seen_for`].
pub fn follow_session(
    client: PandoClient,
    session_id: &str,
    initial_seen: i64,
    config: FollowConfig,
) -> SessionFollow {
    let (tx, rx) = mpsc::channel(512);
    let seen = Arc::new(AtomicI64::new(initial_seen));
    let local_run = Arc::new(AtomicBool::new(false));
    let task = tokio::spawn(run(
        client,
        session_id.to_owned(),
        config,
        tx,
        seen.clone(),
        local_run.clone(),
    ));
    SessionFollow {
        rx,
        seen,
        local_run,
        task,
    }
}

async fn run(
    client: PandoClient,
    sid: String,
    cfg: FollowConfig,
    tx: mpsc::Sender<FollowEvent>,
    seen: Arc<AtomicI64>,
    local_run: Arc<AtomicBool>,
) {
    loop {
        let pending = client.pending(&sid).await;
        let mut again = false;
        if let Ok(p) = pending
            && should_attach(
                seen.load(Ordering::SeqCst),
                false,
                local_run.load(Ordering::SeqCst),
                p.run_seq,
            )
        {
            seen.fetch_max(p.run_seq, Ordering::SeqCst); // claim before opening
            match attach_once(&client, &sid, &tx, &seen).await {
                Attach::Receiver => return,
                Attach::Finished => {}
                // Closed without `done` or failed: reattach if the run is still active.
                Attach::Dropped => again = true,
            }
        }
        tokio::time::sleep(if again {
            cfg.reattach_delay
        } else {
            cfg.poll_interval
        })
        .await;
        if again {
            // The run may still be going: make the next poll attach again.
            if let Ok(p) = client.pending(&sid).await
                && p.running
            {
                seen.fetch_min(p.run_seq - 1, Ordering::SeqCst);
            }
        }
    }
}

enum Attach {
    /// The consumer went away.
    Receiver,
    /// `done` seen.
    Finished,
    /// Stream ended or errored before `done`.
    Dropped,
}

async fn attach_once(
    client: &PandoClient,
    sid: &str,
    tx: &mpsc::Sender<FollowEvent>,
    seen: &AtomicI64,
) -> Attach {
    if tx.send(FollowEvent::ResetBubble).await.is_err() {
        return Attach::Receiver;
    }
    let mut stream = match client.attach_stream(sid).await {
        Ok(s) => s,
        Err(Error::Unauthorized | Error::BasicAuthRequired) => return Attach::Finished,
        Err(_) => return Attach::Dropped,
    };
    let mut content = false;
    loop {
        match stream.next().await {
            Ok(Some(ev)) => {
                if let ChatEvent::Run { run_seq, .. } = &ev {
                    seen.fetch_max(*run_seq, Ordering::SeqCst);
                }
                content |= has_content(&ev);
                let done = ev == ChatEvent::Done;
                if tx.send(FollowEvent::Chat(ev)).await.is_err() {
                    return Attach::Receiver;
                }
                if done {
                    if !content
                        && let Ok(detail) = client.get_session(sid).await
                        && tx
                            .send(FollowEvent::HistoryReload(Box::new(detail)))
                            .await
                            .is_err()
                    {
                        return Attach::Receiver;
                    }
                    if tx.send(FollowEvent::RunFinished).await.is_err() {
                        return Attach::Receiver;
                    }
                    return Attach::Finished;
                }
            }
            Ok(None) | Err(_) => return Attach::Dropped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seen_and_attach_rules() {
        assert_eq!(seen_for(true, 5), -1);
        assert_eq!(seen_for(false, 5), 5);
        assert!(should_attach(-1, false, false, 0));
        assert!(should_attach(1, false, false, 2));
        assert!(!should_attach(2, false, false, 2));
        assert!(!should_attach(1, true, false, 2), "stream already open");
        assert!(!should_attach(1, false, true, 2), "own run in progress");
    }
}
