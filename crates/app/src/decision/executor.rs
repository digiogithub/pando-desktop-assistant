//! Command executor against the Pando API (PDA-T-0101): control decisions run directly
//! against the HTTP API, never through an LLM. Approvals never go to the API from here: they
//! are delegated to the UI thread, which owns the fail-closed ledger and records the voice
//! origin (a flagged request is refused there and reported back).

use std::future::Future;
use std::sync::Arc;

use decide::command::{AgentStatus, Command, CommandExecutor, CommandResult, Targets};
use decide::questions::Engine;
use pando_client::PandoClient;
use pando_client::api::{CreateTask, Project, TaskResponse};

use super::directory::AgentDirectory;
use crate::cells::actions;

/// What the executor needs from Pando (a trait so it is tested without a server).
pub trait Backend: Send + Sync {
    /// `GET /orchestrator/tasks`.
    fn tasks(&self) -> impl Future<Output = Result<Vec<TaskResponse>, String>> + Send;
    /// `POST /orchestrator/tasks/{id}/cancel`.
    fn cancel(&self, id: &str) -> impl Future<Output = Result<(), String>> + Send;
    /// `GET /orchestrator/tasks/{id}`.
    fn task(&self, id: &str) -> impl Future<Output = Result<TaskResponse, String>> + Send;
    /// `POST /orchestrator/tasks`.
    fn create(
        &self,
        body: &CreateTask,
    ) -> impl Future<Output = Result<TaskResponse, String>> + Send;
    /// `GET /projects`.
    fn projects(&self) -> impl Future<Output = Result<Vec<Project>, String>> + Send;
}

impl Backend for PandoClient {
    async fn tasks(&self) -> Result<Vec<TaskResponse>, String> {
        self.list_tasks()
            .await
            .map(|l| l.tasks)
            .map_err(|e| e.to_string())
    }
    async fn cancel(&self, id: &str) -> Result<(), String> {
        actions::cancel(self, id).await.map_err(|e| e.to_string())
    }
    async fn task(&self, id: &str) -> Result<TaskResponse, String> {
        self.get_task(id).await.map_err(|e| e.to_string())
    }
    async fn create(&self, body: &CreateTask) -> Result<TaskResponse, String> {
        actions::create(self, body).await.map_err(|e| e.to_string())
    }
    async fn projects(&self) -> Result<Vec<Project>, String> {
        self.list_projects().await.map_err(|e| e.to_string())
    }
}

/// Outcome of delegating an approval decision to the UI thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VoiceDecisionReport {
    /// Requests resolved.
    pub resolved: usize,
    /// Requests refused because they need an explicit click or key press.
    pub needs_explicit: usize,
}

/// Which pending requests an approval command refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalScope {
    /// The pending requests of these cells.
    Cells(Vec<String>),
    /// Every pending request.
    All,
}

/// The UI side of the executor.
pub trait UiPort: Send + Sync {
    /// Selects a cell (`None`: back to the primary).
    fn open(&self, cell_id: Option<String>);
    /// Approves or denies pending requests with `Origin::Voice`.
    fn decide(
        &self,
        scope: ApprovalScope,
        approve: bool,
    ) -> impl Future<Output = VoiceDecisionReport> + Send;
}

/// Why a command failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExecError {
    /// The named agent is not in the snapshot.
    #[error("unknown agent {0}")]
    UnknownAgent(String),
    /// Pando refused or failed.
    #[error("{0}")]
    Api(String),
    /// Nothing was waiting for an answer.
    #[error("nothing is waiting for an answer")]
    NothingPending,
    /// The pending request needs a click or key press, never a spoken answer.
    #[error("the request needs an explicit approval on screen")]
    NeedsExplicit,
    /// Not available yet (pause/resume until gap G4).
    #[error("not available")]
    Unsupported,
}

/// The executor over a backend and a UI port.
#[allow(missing_debug_implementations)]
pub struct PandoExecutor<B, U> {
    backend: Arc<B>,
    ui: Arc<U>,
    directory: AgentDirectory,
    default_project: Option<String>,
}

impl<B: Backend, U: UiPort> PandoExecutor<B, U> {
    /// Builds an executor for the agents known at decision time.
    pub fn new(backend: Arc<B>, ui: Arc<U>, directory: AgentDirectory) -> Self {
        Self {
            backend,
            ui,
            directory,
            default_project: None,
        }
    }

    /// The project a spawn uses when the sentence names none (the header pill).
    #[must_use]
    pub fn with_default_project(mut self, project: Option<String>) -> Self {
        self.default_project = project;
        self
    }

    fn slug_task(&self, slug: &str) -> Result<String, ExecError> {
        self.directory
            .resolve(slug)
            .map(|e| e.cell_id.clone())
            .ok_or_else(|| ExecError::UnknownAgent(slug.to_owned()))
    }

    fn task_ids(&self, t: &Targets) -> Result<Vec<String>, ExecError> {
        match t {
            Targets::One(slug) => Ok(vec![self.slug_task(slug)?]),
            Targets::All => Ok(self
                .directory
                .entries()
                .iter()
                .filter(|e| e.cell_id != crate::cells::source::PRIMARY_CELL_ID)
                .map(|e| e.cell_id.clone())
                .collect()),
        }
    }

    fn slug_of(&self, task_id: &str) -> String {
        self.directory
            .entries()
            .iter()
            .find(|e| e.cell_id == task_id)
            .map_or_else(|| task_id.to_owned(), |e| e.slug.clone())
    }
}

fn engine_key(e: Engine) -> Option<String> {
    match e {
        Engine::Pando => Some("pando".to_owned()),
        Engine::ClaudeCode => Some("claude".to_owned()),
        Engine::Cli => None,
    }
}

impl<B: Backend, U: UiPort> CommandExecutor for PandoExecutor<B, U> {
    type Error = ExecError;

    async fn execute(&self, command: Command) -> Result<CommandResult, ExecError> {
        match command {
            Command::Cancel { targets } => {
                let ids = self.task_ids(&targets)?;
                let tasks = self.backend.tasks().await.map_err(ExecError::Api)?;
                for id in ids {
                    // A task that already finished is not an error of the command.
                    if tasks.iter().any(|t| t.id == id && t.is_terminal()) {
                        continue;
                    }
                    self.backend.cancel(&id).await.map_err(ExecError::Api)?;
                }
                Ok(CommandResult::Done)
            }
            Command::Status { target } => {
                let tasks = self.backend.tasks().await.map_err(ExecError::Api)?;
                let wanted = match &target {
                    Some(slug) => Some(self.slug_task(slug)?),
                    None => None,
                };
                let agents = tasks
                    .iter()
                    .filter(|t| wanted.as_ref().is_none_or(|w| &t.id == w))
                    .map(|t| AgentStatus {
                        id: self.slug_of(&t.id),
                        status: t.status.clone(),
                        progress: Some(t.progress as f64 / 100.0),
                        current_tool: Some(t.current_tool.clone()).filter(|s| !s.is_empty()),
                    })
                    .collect();
                Ok(CommandResult::Status { agents })
            }
            Command::Open { target } => {
                let cell = target.map(|s| self.slug_task(&s)).transpose()?;
                self.ui.open(cell);
                Ok(CommandResult::Done)
            }
            Command::Retry { targets } => {
                for id in self.task_ids(&targets)? {
                    let t = self.backend.task(&id).await.map_err(ExecError::Api)?;
                    self.backend
                        .create(&actions::relaunch_request(&t))
                        .await
                        .map_err(ExecError::Api)?;
                }
                Ok(CommandResult::Done)
            }
            Command::Approve { targets } => self.answer(&targets, true).await,
            Command::Deny { targets } => self.answer(&targets, false).await,
            Command::Pause { .. } | Command::Resume { .. } => Err(ExecError::Unsupported),
            Command::Spawn {
                engine,
                project,
                prompt,
            } => {
                let work_dir = match project.or_else(|| self.default_project.clone()) {
                    Some(name) => {
                        let projects = self.backend.projects().await.map_err(ExecError::Api)?;
                        let lower = name.to_lowercase();
                        projects
                            .into_iter()
                            .find(|p| p.name.to_lowercase() == lower || p.id == name)
                            .map(|p| p.path)
                    }
                    None => None,
                };
                let body = CreateTask {
                    prompt,
                    work_dir,
                    engine: engine_key(engine),
                    background: true,
                    ..CreateTask::default()
                };
                let t = self.backend.create(&body).await.map_err(ExecError::Api)?;
                Ok(CommandResult::Spawned { id: t.id })
            }
        }
    }
}

impl<B: Backend, U: UiPort> PandoExecutor<B, U> {
    /// Approves or denies pending requests by voice (delegated to the UI thread).
    async fn answer(&self, targets: &Targets, approve: bool) -> Result<CommandResult, ExecError> {
        let scope = match targets {
            Targets::All => ApprovalScope::All,
            Targets::One(slug) => ApprovalScope::Cells(vec![self.slug_task(slug)?]),
        };
        let report = self.ui.decide(scope, approve).await;
        match (report.resolved, report.needs_explicit) {
            (0, 0) => Err(ExecError::NothingPending),
            (0, _) => Err(ExecError::NeedsExplicit),
            _ => Ok(CommandResult::Done),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Fake {
        tasks: Mutex<Vec<TaskResponse>>,
        cancelled: Mutex<Vec<String>>,
        created: Mutex<Vec<CreateTask>>,
    }

    impl Backend for Fake {
        async fn tasks(&self) -> Result<Vec<TaskResponse>, String> {
            Ok(self.tasks.lock().expect("l").clone())
        }
        async fn cancel(&self, id: &str) -> Result<(), String> {
            self.cancelled.lock().expect("l").push(id.to_owned());
            Ok(())
        }
        async fn task(&self, id: &str) -> Result<TaskResponse, String> {
            self.tasks
                .lock()
                .expect("l")
                .iter()
                .find(|t| t.id == id)
                .cloned()
                .ok_or_else(|| "404".to_owned())
        }
        async fn create(&self, body: &CreateTask) -> Result<TaskResponse, String> {
            self.created.lock().expect("l").push(body.clone());
            Ok(TaskResponse {
                id: "new".into(),
                ..TaskResponse::default()
            })
        }
        async fn projects(&self) -> Result<Vec<Project>, String> {
            Ok(vec![Project {
                id: "p1".into(),
                name: "Bitacora".into(),
                path: "/w/bitacora".into(),
                ..Project::default()
            }])
        }
    }

    #[derive(Default)]
    struct Ui {
        opened: Mutex<Vec<Option<String>>>,
        report: Mutex<VoiceDecisionReport>,
        asked: Mutex<Vec<(ApprovalScope, bool)>>,
    }

    impl UiPort for Ui {
        fn open(&self, cell_id: Option<String>) {
            self.opened.lock().expect("l").push(cell_id);
        }
        async fn decide(&self, scope: ApprovalScope, approve: bool) -> VoiceDecisionReport {
            self.asked.lock().expect("l").push((scope, approve));
            *self.report.lock().expect("l")
        }
    }

    fn task(id: &str, status: &str) -> TaskResponse {
        TaskResponse {
            id: id.into(),
            status: status.into(),
            progress: 40,
            current_tool: "edit".into(),
            prompt: "do it".into(),
            ..TaskResponse::default()
        }
    }

    fn fixture() -> (PandoExecutor<Fake, Ui>, Arc<Fake>, Arc<Ui>) {
        let backend = Arc::new(Fake::default());
        *backend.tasks.lock().expect("l") = vec![task("t1", "running"), task("t2", "completed")];
        let ui = Arc::new(Ui::default());
        let dir = AgentDirectory::from_rows([
            (
                "t1".into(),
                "migracion auth".into(),
                "running".into(),
                Some("s1".into()),
            ),
            ("t2".into(), "docs".into(), "completed".into(), None),
        ]);
        (
            PandoExecutor::new(backend.clone(), ui.clone(), dir),
            backend,
            ui,
        )
    }

    fn run<T>(f: impl Future<Output = T>) -> T {
        decide::transport::block_on(f)
    }

    #[test]
    fn cancel_hits_the_api_and_skips_finished_tasks() {
        let (ex, backend, _) = fixture();
        run(ex.execute(Command::Cancel {
            targets: Targets::All,
        }))
        .expect("ok");
        assert_eq!(*backend.cancelled.lock().expect("l"), vec!["t1".to_owned()]);
        assert_eq!(
            run(ex.execute(Command::Cancel {
                targets: Targets::One("nope".into())
            })),
            Err(ExecError::UnknownAgent("nope".into()))
        );
    }

    #[test]
    fn status_is_speakable_data() {
        let (ex, _, _) = fixture();
        let r = run(ex.execute(Command::Status {
            target: Some("migracion-auth".into()),
        }))
        .expect("ok");
        match r {
            CommandResult::Status { agents } => {
                assert_eq!(agents.len(), 1);
                assert_eq!(agents[0].id, "migracion-auth");
                assert_eq!(agents[0].progress, Some(0.4));
                assert_eq!(agents[0].current_tool.as_deref(), Some("edit"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn spawn_resolves_the_project_path_and_engine() {
        let (ex, backend, _) = fixture();
        let r = run(ex.execute(Command::Spawn {
            engine: Engine::ClaudeCode,
            project: Some("bitacora".into()),
            prompt: "revisa los tests".into(),
        }))
        .expect("ok");
        assert_eq!(r, CommandResult::Spawned { id: "new".into() });
        let created = backend.created.lock().expect("l");
        assert_eq!(created[0].work_dir.as_deref(), Some("/w/bitacora"));
        assert_eq!(created[0].engine.as_deref(), Some("claude"));
        assert!(created[0].background);
    }

    #[test]
    fn retry_relaunches_and_open_selects_the_cell() {
        let (ex, backend, ui) = fixture();
        run(ex.execute(Command::Retry {
            targets: Targets::One("docs".into()),
        }))
        .expect("ok");
        assert_eq!(backend.created.lock().expect("l")[0].prompt, "do it");
        run(ex.execute(Command::Open {
            target: Some("migracion-auth".into()),
        }))
        .expect("ok");
        assert_eq!(*ui.opened.lock().expect("l"), vec![Some("t1".to_owned())]);
    }

    #[test]
    fn spoken_approval_is_delegated_and_flagged_requests_are_refused() {
        let (ex, _, ui) = fixture();
        assert_eq!(
            run(ex.execute(Command::Approve {
                targets: Targets::One("migracion-auth".into())
            })),
            Err(ExecError::NothingPending)
        );
        *ui.report.lock().expect("l") = VoiceDecisionReport {
            resolved: 0,
            needs_explicit: 1,
        };
        assert_eq!(
            run(ex.execute(Command::Approve {
                targets: Targets::All
            })),
            Err(ExecError::NeedsExplicit)
        );
        *ui.report.lock().expect("l") = VoiceDecisionReport {
            resolved: 1,
            needs_explicit: 0,
        };
        assert_eq!(
            run(ex.execute(Command::Deny {
                targets: Targets::All
            })),
            Ok(CommandResult::Done)
        );
        let asked = ui.asked.lock().expect("l");
        assert_eq!(asked[2], (ApprovalScope::All, false));
    }

    #[test]
    fn pause_is_not_available() {
        let (ex, _, _) = fixture();
        assert_eq!(
            run(ex.execute(Command::Pause {
                targets: Targets::All
            })),
            Err(ExecError::Unsupported)
        );
    }

    #[test]
    fn commands_run_fast_without_the_network() {
        let (ex, _, _) = fixture();
        let t = std::time::Instant::now();
        run(ex.execute(Command::Status { target: None })).expect("ok");
        assert!(t.elapsed() < std::time::Duration::from_millis(50));
    }
}
