//! Commands and the executor trait (`PDA-SP-0008.R3`): control decisions run directly
//! against the Pando API, never through an LLM.
//!
//! `decide` stays transport-free: [`CommandExecutor`] is implemented by the app on top
//! of `pando-client` (cancel → `POST /orchestrator/tasks/{id}/cancel`, approve/deny →
//! `POST /permissions/respond` or `questions/respond`, spawn → `POST /orchestrator/tasks`,
//! open → select the cell, status → task data). [`dispatch`] only maps a policy-approved
//! [`ControlRequest`] to a [`Command`] and refuses what the server cannot do yet
//! (pause/resume until gap G4), so the caller answers "no disponible" instead of
//! failing silently.

use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::policy::ControlRequest;
use crate::questions::{Action, AgentTarget, Engine, ProjectChoice};

/// Which agents a command acts on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Targets {
    /// One agent by id.
    One(String),
    /// Every live agent.
    All,
}

/// A command for the executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Cancel agent(s).
    Cancel {
        /// Target(s).
        targets: Targets,
    },
    /// Speak the status of one agent, or of all when `None`.
    Status {
        /// Target, or overall.
        target: Option<String>,
    },
    /// Select an agent's cell (or the board when `None`).
    Open {
        /// Target.
        target: Option<String>,
    },
    /// Relaunch agent(s).
    Retry {
        /// Target(s).
        targets: Targets,
    },
    /// Approve the pending item(s) of agent(s) (only after the policy allowed it).
    Approve {
        /// Target(s).
        targets: Targets,
    },
    /// Deny the pending item(s) of agent(s).
    Deny {
        /// Target(s).
        targets: Targets,
    },
    /// Pause agent(s) (needs gap G4).
    Pause {
        /// Target(s).
        targets: Targets,
    },
    /// Resume agent(s) (needs gap G4).
    Resume {
        /// Target(s).
        targets: Targets,
    },
    /// Launch a new agent.
    Spawn {
        /// Engine (Pando when not asked).
        engine: Engine,
        /// Project name; `None` = the current project.
        project: Option<String>,
        /// The transcript, used as the initial prompt.
        prompt: String,
    },
}

impl Command {
    /// The action this command performs.
    pub fn action(&self) -> Action {
        match self {
            Self::Cancel { .. } => Action::Cancel,
            Self::Status { .. } => Action::Status,
            Self::Open { .. } => Action::Open,
            Self::Retry { .. } => Action::Retry,
            Self::Approve { .. } => Action::Approve,
            Self::Deny { .. } => Action::Deny,
            Self::Pause { .. } => Action::Pause,
            Self::Resume { .. } => Action::Resume,
            Self::Spawn { .. } => Action::Spawn,
        }
    }
}

/// What the connected Pando supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Pause/resume endpoints exist (gap G4, `PDA-T-0158`).
    pub pause_resume: bool,
    /// Retry/relaunch endpoint exists.
    pub retry: bool,
}

impl Default for Capabilities {
    /// Today's Pando: no pause/resume over HTTP; retry is a relaunch the app can do.
    fn default() -> Self {
        Self {
            pause_resume: false,
            retry: true,
        }
    }
}

/// Why a request cannot become a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// The server has no endpoint for this action yet: say "no disponible".
    Unsupported(Action),
    /// The request lacks a target the action needs (the policy should have caught it).
    MissingTarget(Action),
}

fn targets(t: &AgentTarget, action: Action) -> Result<Targets, Refusal> {
    match t {
        AgentTarget::Agent(id) => Ok(Targets::One(id.clone())),
        AgentTarget::All => Ok(Targets::All),
        AgentTarget::None => Err(Refusal::MissingTarget(action)),
    }
}

fn single(t: &AgentTarget) -> Option<String> {
    match t {
        AgentTarget::Agent(id) => Some(id.clone()),
        AgentTarget::All | AgentTarget::None => None,
    }
}

/// Map a policy-approved request to a command, refusing unsupported actions.
/// Pure and allocation-light: well under the 50 ms budget.
pub fn to_command(
    request: &ControlRequest,
    transcript: &str,
    caps: Capabilities,
) -> Result<Command, Refusal> {
    let a = request.action;
    let t = &request.target;
    Ok(match a {
        Action::Cancel => Command::Cancel {
            targets: targets(t, a)?,
        },
        Action::Status => Command::Status { target: single(t) },
        Action::Open => Command::Open { target: single(t) },
        Action::Approve => Command::Approve {
            targets: targets(t, a)?,
        },
        Action::Deny => Command::Deny {
            targets: targets(t, a)?,
        },
        Action::Retry if caps.retry => Command::Retry {
            targets: targets(t, a)?,
        },
        Action::Pause if caps.pause_resume => Command::Pause {
            targets: targets(t, a)?,
        },
        Action::Resume if caps.pause_resume => Command::Resume {
            targets: targets(t, a)?,
        },
        Action::Retry | Action::Pause | Action::Resume => return Err(Refusal::Unsupported(a)),
        Action::Spawn => Command::Spawn {
            engine: request.engine.unwrap_or(Engine::Pando),
            project: match &request.project {
                Some(ProjectChoice::Named(p)) => Some(p.clone()),
                Some(ProjectChoice::Current) | None => None,
            },
            prompt: transcript.trim().to_string(),
        },
    })
}

/// Structured status of one agent, from the task data (`TaskResponse`); the app turns
/// it into speech through its locale files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStatus {
    /// Agent id.
    pub id: String,
    /// Status word from the API (`running`, `completed`, ...).
    pub status: String,
    /// Progress in `[0, 1]`, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// Tool in use, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_tool: Option<String>,
}

/// Result of a command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum CommandResult {
    /// Done; nothing to say beyond an acknowledgement.
    Done,
    /// Status data to speak.
    Status {
        /// One entry per agent asked about.
        agents: Vec<AgentStatus>,
    },
    /// A new agent was spawned.
    Spawned {
        /// Its id.
        id: String,
    },
}

/// Executes commands against the Pando API. Implementations never call an LLM.
pub trait CommandExecutor: Send + Sync {
    /// Error type (network, API status...).
    type Error: std::error::Error + Send + Sync + 'static;

    /// Run one command.
    fn execute(
        &self,
        command: Command,
    ) -> impl Future<Output = Result<CommandResult, Self::Error>> + Send;
}

/// Outcome of [`dispatch`].
#[derive(Debug)]
pub enum Dispatched<E> {
    /// The executor ran the command.
    Ran(Result<CommandResult, E>),
    /// Refused before reaching the executor.
    Refused(Refusal),
}

/// Map and run a policy-approved request.
pub async fn dispatch<X: CommandExecutor>(
    executor: &X,
    request: &ControlRequest,
    transcript: &str,
    caps: Capabilities,
) -> Dispatched<X::Error> {
    match to_command(request, transcript, caps) {
        Ok(cmd) => Dispatched::Ran(executor.execute(cmd).await),
        Err(r) => Dispatched::Refused(r),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::transport::block_on;

    fn req(action: Action, target: AgentTarget) -> ControlRequest {
        ControlRequest {
            action,
            target,
            engine: None,
            project: None,
            p: 0.9,
        }
    }

    fn mig() -> AgentTarget {
        AgentTarget::Agent("migracion-auth".into())
    }

    #[derive(Debug, Default)]
    struct Recorder(Mutex<Vec<Command>>);

    #[derive(Debug, thiserror::Error)]
    #[error("never")]
    struct Never;

    impl CommandExecutor for Recorder {
        type Error = Never;
        async fn execute(&self, command: Command) -> Result<CommandResult, Never> {
            let result = match &command {
                Command::Status { target } => CommandResult::Status {
                    agents: vec![AgentStatus {
                        id: target.clone().unwrap_or_default(),
                        status: "running".into(),
                        progress: Some(0.4),
                        current_tool: Some("bash".into()),
                    }],
                },
                Command::Spawn { .. } => CommandResult::Spawned { id: "new-1".into() },
                _ => CommandResult::Done,
            };
            self.0.lock().unwrap().push(command);
            Ok(result)
        }
    }

    #[test]
    fn maps_every_action() {
        let caps = Capabilities {
            pause_resume: true,
            retry: true,
        };
        let one = || Targets::One("migracion-auth".into());
        let cases = [
            (Action::Cancel, Command::Cancel { targets: one() }),
            (Action::Approve, Command::Approve { targets: one() }),
            (Action::Deny, Command::Deny { targets: one() }),
            (Action::Retry, Command::Retry { targets: one() }),
            (Action::Pause, Command::Pause { targets: one() }),
            (Action::Resume, Command::Resume { targets: one() }),
            (
                Action::Status,
                Command::Status {
                    target: Some("migracion-auth".into()),
                },
            ),
            (
                Action::Open,
                Command::Open {
                    target: Some("migracion-auth".into()),
                },
            ),
        ];
        for (a, expected) in cases {
            let cmd = to_command(&req(a, mig()), "x", caps).unwrap();
            assert_eq!(cmd, expected);
            assert_eq!(cmd.action(), a);
        }
        assert_eq!(
            to_command(&req(Action::Cancel, AgentTarget::All), "x", caps),
            Ok(Command::Cancel {
                targets: Targets::All
            })
        );
        assert_eq!(
            to_command(&req(Action::Status, AgentTarget::None), "x", caps),
            Ok(Command::Status { target: None })
        );
    }

    #[test]
    fn pause_resume_unavailable_until_g4() {
        let caps = Capabilities::default();
        for a in [Action::Pause, Action::Resume] {
            assert_eq!(
                to_command(&req(a, mig()), "pausa", caps),
                Err(Refusal::Unsupported(a))
            );
        }
        let caps = Capabilities {
            retry: false,
            ..caps
        };
        assert_eq!(
            to_command(&req(Action::Retry, mig()), "x", caps),
            Err(Refusal::Unsupported(Action::Retry))
        );
    }

    #[test]
    fn missing_targets_are_refused() {
        for a in [Action::Cancel, Action::Approve, Action::Deny] {
            assert_eq!(
                to_command(&req(a, AgentTarget::None), "x", Capabilities::default()),
                Err(Refusal::MissingTarget(a))
            );
        }
    }

    #[test]
    fn spawn_uses_engine_project_and_transcript() {
        let mut r = req(Action::Spawn, AgentTarget::None);
        r.engine = Some(Engine::ClaudeCode);
        r.project = Some(ProjectChoice::Named("bitacora".into()));
        assert_eq!(
            to_command(&r, " lanza un claude en bitácora ", Capabilities::default()),
            Ok(Command::Spawn {
                engine: Engine::ClaudeCode,
                project: Some("bitacora".into()),
                prompt: "lanza un claude en bitácora".into(),
            })
        );
        r.engine = None;
        r.project = Some(ProjectChoice::Current);
        let Ok(Command::Spawn {
            engine, project, ..
        }) = to_command(&r, "lanza", Capabilities::default())
        else {
            panic!("spawn expected");
        };
        assert_eq!((engine, project), (Engine::Pando, None));
    }

    #[test]
    fn dispatch_runs_or_refuses_fast() {
        let x = Recorder::default();
        let started = Instant::now();
        let out = block_on(dispatch(
            &x,
            &req(Action::Status, mig()),
            "¿cómo va migración auth?",
            Capabilities::default(),
        ));
        // Acceptance: < 50 ms after the decision, excluding network.
        assert!(started.elapsed() < Duration::from_millis(50));
        match out {
            Dispatched::Ran(Ok(CommandResult::Status { agents })) => {
                assert_eq!(agents[0].progress, Some(0.4));
            }
            other => panic!("{other:?}"),
        }
        let out = block_on(dispatch(
            &x,
            &req(Action::Pause, mig()),
            "pausa",
            Capabilities::default(),
        ));
        assert!(matches!(
            out,
            Dispatched::Refused(Refusal::Unsupported(Action::Pause))
        ));
        assert_eq!(x.0.lock().unwrap().len(), 1, "refused commands never run");
    }
}
