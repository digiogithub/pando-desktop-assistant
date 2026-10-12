//! Claude Code sessions in the organism (PDA-T-0132, PDA-T-0134): the hook receiver feeds a
//! [`SessionTracker`]; every session becomes a `ClaudeExternal` cell (hollow blue when born,
//! `?` while it waits for the user, a Marfil nucleus when done); "avísame al terminar" queues
//! the Stop summary to be spoken; messages are sent over the session's messaging socket.

pub mod extras;
pub mod tell;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use claude_bridge::{
    CellStatus, CellUpdate, Delivery, ExternalCell, HookEvent, HookInstaller, HookServer,
    InstallReport, ProjectRef, RuntimeFiles, SessionTracker, send_message,
};
use organism::{CellInput, Engine, Status};
use rust_i18n::t;
use speech::Priority;

use crate::cells::pane::OrganismPane;
use crate::events::EventPump;
use crate::tokio_bridge;
use crate::ui::{Context, Entity, EventEmitter, Task, WeakEntity};
use crate::voice::assistant::Assistant;

/// Cell id prefix of external sessions.
pub const CELL_PREFIX: &str = "claude:";

/// The organism cell of a session.
#[must_use]
pub fn cell_for(c: &ExternalCell) -> CellInput {
    let status = match c.status {
        CellStatus::Born | CellStatus::Working => Status::Working,
        CellStatus::Waiting => Status::Waiting,
        CellStatus::Done => Status::Done,
    };
    let mut cell = CellInput::new(
        &format!("{CELL_PREFIX}{}", c.session_id),
        Engine::ClaudeExternal,
        status,
    );
    cell.is_new = c.status == CellStatus::Born;
    cell.label = c.label.as_str().into();
    cell
}

/// The session id behind a cell id.
#[must_use]
pub fn session_of_cell(cell_id: &str) -> Option<&str> {
    cell_id.strip_prefix(CELL_PREFIX)
}

/// State of the hook receiver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeStatus {
    /// Binding.
    Starting,
    /// Listening on this port.
    Listening(u16),
    /// Could not start (reason).
    Failed(String),
}

/// What the bridge reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeEvent {
    /// A message was handed to a session (or held) / failed; text for a toast.
    Message(String),
}

#[derive(Debug)]
enum Msg {
    Bound(u16),
    BindFailed(String),
    Hook(Box<HookEvent>),
    Projects(Vec<ProjectRef>),
    Sent {
        label: String,
        result: Result<Delivery, String>,
    },
}

/// The bridge entity.
#[allow(missing_debug_implementations)]
pub struct ClaudeBridge {
    tracker: SessionTracker,
    pane: Entity<OrganismPane>,
    assistant: WeakEntity<Assistant>,
    epoch: Instant,
    tx: async_channel::Sender<Msg>,
    runtime: RuntimeFiles,
    data_dir: PathBuf,
    /// Receiver state.
    pub status: BridgeStatus,
    _shutdown: async_channel::Sender<()>,
    _pump: Task<()>,
    _ticker: Task<()>,
}

impl EventEmitter<BridgeEvent> for ClaudeBridge {}

impl ClaudeBridge {
    /// Starts the receiver in `<data_dir>/claude-bridge` and the periodic upkeep.
    pub fn new(
        cx: &mut Context<Self>,
        pane: Entity<OrganismPane>,
        assistant: WeakEntity<Assistant>,
        data_dir: PathBuf,
    ) -> Self {
        let (tx, rx) = async_channel::unbounded();
        let (shutdown_tx, shutdown_rx) = async_channel::bounded::<()>(1);
        let runtime = RuntimeFiles::new(data_dir.join("claude-bridge"));
        let pump = EventPump::attach(cx, rx, |this: &mut Self, msg, cx| this.apply(msg, cx));
        // The receiver lives on the tokio bridge (not started in unit tests: the GPUI test
        // scheduler forbids foreign threads).
        if !cfg!(test) {
            let (tx, runtime) = (tx.clone(), runtime.clone());
            tokio_bridge::spawn(cx, async move {
                let server = match HookServer::bind(runtime).await {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = tx.send(Msg::BindFailed(e.to_string())).await;
                        return;
                    }
                };
                let _ = tx.send(Msg::Bound(server.port())).await;
                let events = server.events();
                let forward = {
                    let tx = tx.clone();
                    async move {
                        while let Ok(ev) = events.recv().await {
                            if tx.send(Msg::Hook(Box::new(ev))).await.is_err() {
                                break;
                            }
                        }
                    }
                };
                let serve = server.serve(async move {
                    let _ = shutdown_rx.recv().await;
                });
                tokio::select! {
                    _ = forward => {}
                    r = serve => {
                        if let Err(e) = r {
                            tracing::warn!("claude hook receiver stopped: {e}");
                        }
                    }
                }
            })
            .detach();
        }
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                if this.update(cx, |this, cx| this.upkeep(cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            tracker: SessionTracker::new(),
            pane,
            assistant,
            epoch: Instant::now(),
            tx,
            runtime,
            data_dir,
            status: BridgeStatus::Starting,
            _shutdown: shutdown_tx,
            _pump: pump,
            _ticker: ticker,
        }
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn upkeep(&mut self, cx: &mut Context<Self>) {
        let updates = self.tracker.expire(self.now_ms());
        self.handle_updates(updates, cx);
        let Some(client) = self.pane.read(cx).voice_client() else {
            return;
        };
        {
            let tx = self.tx.clone();
            tokio_bridge::spawn(cx, async move {
                if let Ok(projects) = client.list_projects().await {
                    let refs = projects
                        .into_iter()
                        .filter(|p| !p.path.is_empty())
                        .map(|p| ProjectRef {
                            id: p.id,
                            path: PathBuf::from(p.path),
                        })
                        .collect();
                    let _ = tx.send(Msg::Projects(refs)).await;
                }
            })
            .detach();
        }
    }

    fn apply(&mut self, msg: Msg, cx: &mut Context<Self>) {
        match msg {
            Msg::Bound(port) => self.status = BridgeStatus::Listening(port),
            Msg::BindFailed(why) => {
                tracing::warn!("claude hook receiver could not start: {why}");
                self.status = BridgeStatus::Failed(why);
            }
            Msg::Hook(ev) => {
                let updates = self.tracker.apply(&ev, self.now_ms());
                self.handle_updates(updates, cx);
            }
            Msg::Projects(p) => {
                let updates = self.tracker.set_projects(p);
                self.handle_updates(updates, cx);
            }
            Msg::Sent { label, result } => {
                let text = match result {
                    Ok(Delivery::Held) => t!("claude.sent_held", session = label).to_string(),
                    Ok(_) => t!("claude.sent", session = label).to_string(),
                    Err(why) => t!("claude.send_failed", session = label, why = why).to_string(),
                };
                cx.emit(BridgeEvent::Message(text.clone()));
                self.say(text, Priority::Direct, None, cx);
            }
        }
        cx.notify();
    }

    fn handle_updates(&mut self, updates: Vec<CellUpdate>, cx: &mut Context<Self>) {
        let mut speak = Vec::new();
        for u in updates {
            if let CellUpdate::Speak {
                session_id,
                label,
                text,
            } = u
            {
                speak.push((session_id, label, text));
            }
        }
        let cells: Vec<CellInput> = self.tracker.cells().map(cell_for).collect();
        self.pane
            .update(cx, |p, cx| p.set_external_cells(cells, cx));
        for (session, label, text) in speak {
            let line = crate::voice::speak::claude_stop_speech(&label, &text);
            self.say(
                line,
                Priority::Conclusion,
                Some(format!("{CELL_PREFIX}{session}")),
                cx,
            );
        }
    }

    fn say(
        &self,
        text: String,
        priority: Priority,
        source: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let _ = self
            .assistant
            .update(cx, |a, cx| a.notify(text, priority, source, cx));
    }

    /// The tracked sessions (for the card and voice lookups).
    pub fn sessions(&self) -> Vec<ExternalCell> {
        self.tracker.cells().cloned().collect()
    }

    /// The session whose label best matches `spoken` (normalized containment either way).
    #[must_use]
    pub fn find_session(&self, spoken: &str) -> Option<ExternalCell> {
        tell::best_session(&self.sessions(), spoken)
    }

    /// "Avísame al terminar" on/off for a session.
    pub fn set_notify(&mut self, session_id: &str, on: bool, cx: &mut Context<Self>) -> bool {
        let changed = self.tracker.set_notify(session_id, on);
        cx.notify();
        changed
    }

    /// Whether "avísame" is armed for a session.
    #[must_use]
    pub fn notify_armed(&self, session_id: &str) -> bool {
        self.tracker
            .get(session_id)
            .is_some_and(|c| c.notify_when_done)
    }

    /// Sends `text` to a session over its messaging socket (off the UI thread). The outcome
    /// is reported as a toast and spoken.
    pub fn send(&mut self, session_id: &str, text: String, cx: &mut Context<Self>) {
        let Some(cell) = self.tracker.get(session_id).cloned() else {
            return;
        };
        let label = cell.label.clone();
        let tx = self.tx.clone();
        let Some(endpoint) = cell.messaging else {
            let _ = tx.try_send(Msg::Sent {
                label,
                result: Err(t!("claude.no_socket").to_string()),
            });
            return;
        };
        std::thread::spawn(move || {
            let result = send_message(&endpoint, &text).map_err(|e| e.to_string());
            let _ = tx.send_blocking(Msg::Sent { label, result });
        });
        let _ = cx;
    }

    /// Installs the hooks into `~/.claude/settings.json` (first-run step, with consent).
    pub fn install_hooks(&self) -> Result<InstallReport, String> {
        let settings = HookInstaller::default_settings_path().ok_or("no home directory")?;
        HookInstaller::new(settings, &self.data_dir)
            .install(&self.runtime)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use claude_bridge::{CellStatus, ExternalCell};

    use super::*;

    fn cell(status: CellStatus) -> ExternalCell {
        ExternalCell {
            session_id: "abc".into(),
            label: "api".into(),
            cwd: "/w/api".into(),
            project_id: None,
            status,
            summary: None,
            waiting_message: None,
            messaging: None,
            notify_when_done: false,
        }
    }

    #[test]
    fn statuses_map_to_cells() {
        let born = cell_for(&cell(CellStatus::Born));
        assert_eq!(born.id.as_str(), "claude:abc");
        assert!(born.is_new);
        assert_eq!(born.engine, Engine::ClaudeExternal);
        assert_eq!(cell_for(&cell(CellStatus::Waiting)).status, Status::Waiting);
        assert_eq!(cell_for(&cell(CellStatus::Done)).status, Status::Done);
        assert!(!cell_for(&cell(CellStatus::Working)).is_new);
        assert_eq!(session_of_cell("claude:abc"), Some("abc"));
        assert_eq!(session_of_cell("task-1"), None);
    }
}
