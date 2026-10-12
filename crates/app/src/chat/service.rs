//! The chat service (PDA-T-0068, PDA-T-0072): everything that talks to Pando on the tokio
//! runtime and reports back through one `async_channel` of [`ChatMsg`], drained by an
//! [`crate::events::EventPump`] (one notification per burst of SSE deltas).
//!
//! [`ChatService`] is cheap to clone and usable from any thread. Every message that belongs to
//! one session carries the model's generation (`epoch`): the panel drops messages of a session it
//! has left, so a late history response or a stream of the previous session can never leak into
//! the current transcript.

use std::collections::BTreeSet;
use std::time::Duration;

use async_channel::Sender;
use pando_client::PandoClient;
use pando_client::api::{ActiveModel, CommandInfo, ModelList};
use pando_client::approvals::Outgoing;
use pando_client::chat::{ChatEvent, Goal, SessionDetail, SessionList, SessionModel};
use pando_client::follow::{FollowConfig, FollowEvent, follow_session};
use pando_client::steering::{ChipId, SteerResult, StopPlan, execute_stop, send_steer};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use super::composer::files_from_search;
use super::sessions::PAGE_SIZE;

/// How a session page is merged into the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageMode {
    /// First page: replace everything.
    Replace,
    /// Refresh of the first page: keep the pages loaded below.
    Head,
    /// Next page.
    Append,
}

/// Everything the service reports.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ChatMsg {
    /// A page of the session list.
    Sessions {
        /// How to merge it.
        mode: PageMode,
        /// The page or the error text.
        result: Result<SessionList, String>,
    },
    /// The history of a session being opened.
    Opened {
        /// Generation of the open request.
        epoch: u64,
        /// History or error text.
        result: Result<SessionDetail, String>,
    },
    /// The goal of a session being opened.
    OpenedGoal {
        /// Generation.
        epoch: u64,
        /// Goal.
        goal: Option<Goal>,
    },
    /// Pending approvals of a session being opened (replayed so none is missed).
    OpenedPending {
        /// Generation.
        epoch: u64,
        /// Pending permissions and questions as events.
        events: Vec<ChatEvent>,
    },
    /// An event of the app's own chat stream.
    Chat {
        /// Generation.
        epoch: u64,
        /// Which of the app's own runs (a late message of an older run is dropped).
        run: u64,
        /// The event.
        ev: ChatEvent,
    },
    /// The app's own chat stream ended.
    ChatEnded {
        /// Generation.
        epoch: u64,
        /// Which of the app's own runs.
        run: u64,
        /// Why it failed, if it did.
        error: Option<String>,
    },
    /// An event of the follower (resurrected or foreign runs).
    Follow {
        /// Generation.
        epoch: u64,
        /// The event.
        ev: FollowEvent,
    },
    /// Outcome of a steer request.
    Steer {
        /// Generation.
        epoch: u64,
        /// The chip.
        chip: ChipId,
        /// Outcome.
        result: Result<SteerResult, String>,
    },
    /// Stop finished.
    Stopped {
        /// Generation.
        epoch: u64,
        /// Error text.
        error: Option<String>,
    },
    /// Approval answers sent to Pando.
    Delivered {
        /// Every id that was attempted.
        sent: Vec<String>,
        /// The ones that got through.
        ok: Vec<String>,
    },
    /// `GET /models`.
    Models(Result<ModelList, String>),
    /// `GET /commands`.
    Commands(Result<Vec<CommandInfo>, String>),
    /// The effective model of the open session.
    SessionModel {
        /// Generation.
        epoch: u64,
        /// Model or error.
        result: Result<SessionModel, String>,
    },
    /// Result of `PUT /models/active` for a session.
    ModelSet {
        /// Generation.
        epoch: u64,
        /// The requested model.
        model: String,
        /// Answer.
        result: Result<ActiveModel, String>,
    },
    /// Auto-approve state: read (`requested == None`) or after a toggle.
    AutoApprove {
        /// Generation.
        epoch: u64,
        /// What the user asked for.
        requested: Option<bool>,
        /// Server state or error.
        result: Result<bool, String>,
    },
    /// File search results for `@` mentions.
    Files {
        /// The query.
        query: String,
        /// Paths or error.
        result: Result<Vec<String>, String>,
    },
    /// Rename outcome.
    Renamed {
        /// Session.
        id: String,
        /// New title.
        title: String,
        /// Error text.
        error: Option<String>,
    },
    /// Delete outcome.
    Deleted {
        /// Session.
        id: String,
        /// Error text.
        error: Option<String>,
    },
    /// Pending items per running session, from the background poll.
    PendingPoll(Vec<(String, usize)>),
}

/// Aborts a tokio task when dropped.
#[derive(Debug)]
pub struct TaskGuard(AbortHandle);

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Commands to a follower task.
#[derive(Debug)]
enum FollowCmd {
    BeginLocalRun,
    Observe(Box<ChatEvent>),
}

/// Controls the follower of one session; dropping it stops following.
#[derive(Debug)]
pub struct FollowHandle {
    cmd: mpsc::UnboundedSender<FollowCmd>,
    _guard: TaskGuard,
}

impl FollowHandle {
    /// The app is starting its own run: the poller must not attach to it as well.
    pub fn begin_local_run(&self) {
        let _ = self.cmd.send(FollowCmd::BeginLocalRun);
    }

    /// Feeds an event of the app's own stream (advances `seenRunSeq`, ends the local run).
    pub fn observe(&self, ev: &ChatEvent) {
        let _ = self.cmd.send(FollowCmd::Observe(Box::new(ev.clone())));
    }
}

/// Client-side handle on Pando for the chat.
#[derive(Debug, Clone)]
pub struct ChatService {
    client: PandoClient,
    handle: Handle,
    tx: Sender<ChatMsg>,
}

/// Period of the background poll (sessions list + pending counts).
pub const POLL_INTERVAL: Duration = Duration::from_secs(4);
/// Running sessions inspected per poll.
const POLL_MAX_SESSIONS: usize = 24;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl ChatService {
    /// A service sending its results on `tx`.
    #[must_use]
    pub fn new(client: PandoClient, handle: Handle, tx: Sender<ChatMsg>) -> Self {
        Self { client, handle, tx }
    }

    /// The underlying client.
    #[must_use]
    pub fn client(&self) -> &PandoClient {
        &self.client
    }

    fn spawn<F>(&self, fut: F) -> TaskGuard
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        TaskGuard(self.handle.spawn(fut).abort_handle())
    }

    fn detach<F>(&self, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        drop(self.handle.spawn(fut));
    }

    /// Loads a page of sessions.
    pub fn list_sessions(&self, mode: PageMode, offset: u32) {
        let (client, tx) = (self.client.clone(), self.tx.clone());
        self.detach(async move {
            let result = client.list_sessions(PAGE_SIZE, offset).await.map_err(err);
            let _ = tx.send(ChatMsg::Sessions { mode, result }).await;
        });
    }

    /// Loads everything needed to show a session: history first (so the transcript appears as
    /// soon as possible), then goal, pending approvals, model and auto-approve state.
    pub fn open_session(&self, epoch: u64, id: &str) {
        let (client, tx, id) = (self.client.clone(), self.tx.clone(), id.to_owned());
        self.detach(async move {
            let history = client.get_session(&id).await.map_err(err);
            let _ = tx
                .send(ChatMsg::Opened {
                    epoch,
                    result: history,
                })
                .await;
            if let Ok(goal) = client.goal(&id).await {
                let _ = tx.send(ChatMsg::OpenedGoal { epoch, goal }).await;
            }
            if let Ok(p) = client.pending(&id).await {
                let mut events: Vec<ChatEvent> = Vec::new();
                events.extend(p.permissions.into_iter().map(ChatEvent::PermissionRequest));
                events.extend(p.questions.into_iter().map(ChatEvent::QuestionRequest));
                if !events.is_empty() {
                    let _ = tx.send(ChatMsg::OpenedPending { epoch, events }).await;
                }
            }
            let result = client.session_model(&id).await.map_err(err);
            let _ = tx.send(ChatMsg::SessionModel { epoch, result }).await;
            let result = client.auto_approve(&id).await.map_err(err);
            let _ = tx
                .send(ChatMsg::AutoApprove {
                    epoch,
                    requested: None,
                    result,
                })
                .await;
        });
    }

    /// Sends `prompt` as a normal run (creating the session when `session_id` is `None`) and
    /// streams its events. Dropping the guard closes the stream; the run survives on the server.
    pub fn send_prompt(
        &self,
        epoch: u64,
        run: u64,
        session_id: Option<String>,
        prompt: String,
        model: Option<String>,
    ) -> TaskGuard {
        let (client, tx) = (self.client.clone(), self.tx.clone());
        self.spawn(async move {
            let mut stream = match client
                .chat_stream(session_id.as_deref(), &prompt, model.as_deref())
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx
                        .send(ChatMsg::ChatEnded {
                            epoch,
                            run,
                            error: Some(err(e)),
                        })
                        .await;
                    return;
                }
            };
            loop {
                match stream.next().await {
                    Ok(Some(ev)) => {
                        if tx.send(ChatMsg::Chat { epoch, run, ev }).await.is_err() {
                            return;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = tx
                            .send(ChatMsg::ChatEnded {
                                epoch,
                                run,
                                error: Some(err(e)),
                            })
                            .await;
                        return;
                    }
                }
            }
            let _ = tx
                .send(ChatMsg::ChatEnded {
                    epoch,
                    run,
                    error: None,
                })
                .await;
        })
    }

    /// Starts following a session for runs this app did not start.
    pub fn follow(&self, epoch: u64, id: &str, initial_seen: i64) -> FollowHandle {
        let (client, tx, id) = (self.client.clone(), self.tx.clone(), id.to_owned());
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let guard = self.spawn(async move {
            let mut follow = follow_session(client, &id, initial_seen, FollowConfig::default());
            loop {
                tokio::select! {
                    ev = follow.next() => {
                        let Some(ev) = ev else { return };
                        if tx.send(ChatMsg::Follow { epoch, ev }).await.is_err() {
                            return;
                        }
                    }
                    cmd = cmd_rx.recv() => match cmd {
                        Some(FollowCmd::BeginLocalRun) => follow.begin_local_run(),
                        Some(FollowCmd::Observe(ev)) => follow.observe(&ev),
                        None => return,
                    },
                }
            }
        });
        FollowHandle {
            cmd: cmd_tx,
            _guard: guard,
        }
    }

    /// Steers the running session with `text`.
    pub fn steer(&self, epoch: u64, session_id: &str, chip: ChipId, text: &str) {
        let (client, tx) = (self.client.clone(), self.tx.clone());
        let (id, text) = (session_id.to_owned(), text.to_owned());
        self.detach(async move {
            let result = send_steer(&client, &id, &text).await.map_err(err);
            let _ = tx
                .send(ChatMsg::Steer {
                    epoch,
                    chip,
                    result,
                })
                .await;
        });
    }

    /// Stop: cancels the run and, when a goal is active, the goal.
    pub fn stop(&self, epoch: u64, session_id: &str, plan: StopPlan) {
        let (client, tx, id) = (self.client.clone(), self.tx.clone(), session_id.to_owned());
        self.detach(async move {
            let error = execute_stop(&client, &id, plan).await.err().map(err);
            let _ = tx.send(ChatMsg::Stopped { epoch, error }).await;
        });
    }

    /// Delivers resolved approvals; reports the ids that got through.
    pub fn deliver(&self, outbox: Vec<Outgoing>) {
        if outbox.is_empty() {
            return;
        }
        let (client, tx) = (self.client.clone(), self.tx.clone());
        self.detach(async move {
            let mut done = Vec::new();
            let mut sent = Vec::new();
            for o in outbox {
                let (id, res) = match o {
                    Outgoing::Permission {
                        id,
                        session_id,
                        action,
                    } => {
                        let r = client.permissions_respond(&id, &session_id, action).await;
                        (id, r.map(|_| ()))
                    }
                    Outgoing::Question {
                        id,
                        session_id,
                        answers,
                        cancelled,
                    } => {
                        let r = client
                            .questions_respond(&id, &session_id, &answers, cancelled)
                            .await;
                        (id, r.map(|_| ()))
                    }
                };
                sent.push(id.clone());
                if res.is_ok() {
                    done.push(id);
                } else {
                    tracing::warn!("could not deliver an approval answer; it stays queued");
                }
            }
            let _ = tx.send(ChatMsg::Delivered { sent, ok: done }).await;
        });
    }

    /// Loads the model list.
    pub fn list_models(&self) {
        let (client, tx) = (self.client.clone(), self.tx.clone());
        self.detach(async move {
            let _ = tx
                .send(ChatMsg::Models(client.list_models().await.map_err(err)))
                .await;
        });
    }

    /// Loads the slash commands.
    pub fn list_commands(&self) {
        let (client, tx) = (self.client.clone(), self.tx.clone());
        self.detach(async move {
            let _ = tx
                .send(ChatMsg::Commands(client.list_commands().await.map_err(err)))
                .await;
        });
    }

    /// Sets the model **of one session**. There is deliberately no call that changes the
    /// global model: `PUT /models/active` is only ever sent with a `sessionId`.
    pub fn set_session_model(&self, epoch: u64, session_id: &str, model: &str) {
        let (client, tx) = (self.client.clone(), self.tx.clone());
        let (id, model) = (session_id.to_owned(), model.to_owned());
        self.detach(async move {
            let result = client
                .set_active_model(&model, Some(&id))
                .await
                .map_err(err);
            let _ = tx
                .send(ChatMsg::ModelSet {
                    epoch,
                    model,
                    result,
                })
                .await;
        });
    }

    /// Switches auto-approve of a session on the server.
    pub fn set_auto_approve(&self, epoch: u64, session_id: &str, enabled: bool) {
        let (client, tx, id) = (self.client.clone(), self.tx.clone(), session_id.to_owned());
        self.detach(async move {
            let result = client.set_auto_approve(&id, enabled).await.map_err(err);
            let _ = tx
                .send(ChatMsg::AutoApprove {
                    epoch,
                    requested: Some(enabled),
                    result,
                })
                .await;
        });
    }

    /// Searches project files for `@` mentions.
    pub fn search_files(&self, query: &str) {
        let (client, tx, query) = (self.client.clone(), self.tx.clone(), query.to_owned());
        self.detach(async move {
            let result = client
                .files_search(&query)
                .await
                .map(|v| files_from_search(&v, 20))
                .map_err(err);
            let _ = tx.send(ChatMsg::Files { query, result }).await;
        });
    }

    /// Renames a session.
    pub fn rename(&self, id: &str, title: &str) {
        let (client, tx) = (self.client.clone(), self.tx.clone());
        let (id, title) = (id.to_owned(), title.to_owned());
        self.detach(async move {
            let error = client.rename_session(&id, &title).await.err().map(err);
            let _ = tx.send(ChatMsg::Renamed { id, title, error }).await;
        });
    }

    /// Deletes a session.
    pub fn delete(&self, id: &str) {
        let (client, tx, id) = (self.client.clone(), self.tx.clone(), id.to_owned());
        self.detach(async move {
            let error = client.delete_session(&id).await.err().map(err);
            let _ = tx.send(ChatMsg::Deleted { id, error }).await;
        });
    }

    /// Background poll: refreshes the first page of sessions and counts pending approvals of
    /// the running ones (the global indicator, PDA-T-0083). Dropping the guard stops it.
    pub fn start_poller(&self) -> TaskGuard {
        let (client, tx) = (self.client.clone(), self.tx.clone());
        self.spawn(async move {
            loop {
                match client.list_sessions(PAGE_SIZE, 0).await {
                    Ok(list) => {
                        let running: Vec<String> = list
                            .sessions
                            .iter()
                            .filter(|s| s.is_running)
                            .take(POLL_MAX_SESSIONS)
                            .map(|s| s.session.id.clone())
                            .collect();
                        if tx
                            .send(ChatMsg::Sessions {
                                mode: PageMode::Head,
                                result: Ok(list),
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                        let mut counts = Vec::new();
                        let mut seen = BTreeSet::new();
                        for id in running {
                            if let Ok(p) = client.pending(&id).await {
                                counts.push((id.clone(), p.permissions.len() + p.questions.len()));
                            }
                            seen.insert(id);
                        }
                        if tx.send(ChatMsg::PendingPoll(counts)).await.is_err() {
                            return;
                        }
                    }
                    Err(e) => tracing::debug!("session poll failed: {e}"),
                }
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        })
    }
}
