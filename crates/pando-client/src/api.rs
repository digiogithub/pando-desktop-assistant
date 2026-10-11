//! Orchestrator, instances, projects, models, commands and misc REST calls.
//!
//! `TaskResponse` carries the fields Pando has always sent plus the optional additive ones of
//! gap G2 (`PDA-T-0156`): every extra field is `Option`/default so older servers still parse.
//! There is no task push stream (G1): poll [`PandoClient::list_tasks`] every 1-2 s and feed the
//! result to a [`TaskTracker`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::PandoClient;
use crate::conclusion::Conclusion;
use crate::error::Result;
use crate::sse::SseEvent;
use crate::stream::{EventStream, EventStreamConfig};

// ---------------------------------------------------------------- orchestrator

/// ACP session state of a task (G2).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AcpStatus {
    /// ACP session id.
    pub session_id: String,
    /// Mode.
    pub mode: String,
    /// Agent name.
    pub agent_name: String,
    /// Connected.
    pub is_connected: bool,
    /// Tool calls so far.
    pub tool_calls: i64,
    /// Last activity.
    pub last_activity: String,
}

/// A tool call of a task.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TaskToolCall {
    /// Call id.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Title.
    pub title: String,
    /// Kind.
    pub kind: String,
    /// Arguments.
    pub arguments: Option<Value>,
    /// Result text.
    pub result: String,
    /// Status.
    pub status: String,
}

/// `TaskResponse`. The G2 block at the end is present only on servers with `PDA-T-0156`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TaskResponse {
    /// Task id.
    pub id: String,
    /// First 80 chars of the prompt.
    pub name: String,
    /// Prompt.
    pub prompt: String,
    /// Engine (`claude|pando|acp-*`).
    pub agent: String,
    /// Model.
    pub model: String,
    /// Persona.
    pub persona: String,
    /// `pending|running|paused|completed|failed|cancelled`.
    pub status: String,
    /// Percent.
    pub progress: i64,
    /// Always 0 on current servers.
    pub tokens: i64,
    /// Output so far.
    pub output: String,
    /// Tool in use.
    pub current_tool: String,
    /// Tool calls.
    #[serde(deserialize_with = "null_vec")]
    pub tool_calls: Vec<TaskToolCall>,
    /// RFC 3339.
    pub created_at: String,
    /// RFC 3339.
    pub updated_at: String,
    // ---- G2 (optional)
    /// Working directory.
    pub work_dir: Option<String>,
    /// Tags.
    pub tags: Option<Vec<String>>,
    /// Error text.
    pub error: Option<String>,
    /// Exit code.
    pub exit_code: Option<i64>,
    /// Last output lines.
    pub output_tail: Option<String>,
    /// RFC 3339.
    pub started_at: Option<String>,
    /// RFC 3339.
    pub completed_at: Option<String>,
    /// Dependencies.
    pub dependencies: Option<Vec<String>>,
    /// ACP session.
    pub acp_session_id: Option<String>,
    /// ACP mode.
    pub acp_mode: Option<String>,
    /// ACP state ("waiting for permission" detection).
    pub acp_status: Option<AcpStatus>,
    /// Chat session that spawned it.
    pub parent_session_id: Option<String>,
    /// Project id.
    pub project_id: Option<String>,
    /// Project path.
    pub project_path: Option<String>,
    /// Project name.
    pub project_name: Option<String>,
    /// Structured conclusion.
    pub conclusion: Option<Conclusion>,
}

fn null_vec<'de, D, T>(d: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

impl TaskResponse {
    /// The task reached a final state.
    pub fn is_terminal(&self) -> bool {
        matches!(self.status.as_str(), "completed" | "failed" | "cancelled")
    }
}

/// `GET /orchestrator/tasks`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TaskList {
    /// Tasks.
    #[serde(deserialize_with = "null_vec")]
    pub tasks: Vec<TaskResponse>,
    /// Count.
    pub total: i64,
}

/// Body of `POST /orchestrator/tasks`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CreateTask {
    /// Prompt (required).
    pub prompt: String,
    /// Working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_dir: Option<String>,
    /// Model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Engine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// Tags.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Dependencies.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    /// Forced to true by the server.
    pub background: bool,
}

/// `GET /orchestrator/delegation/metrics`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct DelegationMetrics {
    /// Warm attempts.
    pub warm_attempts: i64,
    /// Warm hits.
    pub warm_hits: i64,
    /// Resurrections of idle parents.
    pub resurrections: i64,
    /// Live injections.
    pub live_injections: i64,
    /// Hit rate.
    pub warm_hit_rate: f64,
}

/// What changed between two polls of the task list.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskChange {
    /// A new task (cell birth).
    Appeared(TaskResponse),
    /// Status or progress changed.
    Updated(TaskResponse),
    /// A task reached a terminal state (cell death); also reported as appeared if first seen so.
    Finished(TaskResponse),
    /// The task vanished from the list (deleted).
    Removed(String),
}

/// Diffs successive polls by `id` + `status` + `progress`.
#[derive(Debug, Default)]
pub struct TaskTracker {
    seen: HashMap<String, (String, i64)>,
}

impl TaskTracker {
    /// A tracker that has seen nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one poll, returns the changes since the previous one.
    pub fn update(&mut self, tasks: &[TaskResponse]) -> Vec<TaskChange> {
        let mut out = Vec::new();
        let mut next = HashMap::new();
        for t in tasks {
            let key = (t.status.clone(), t.progress);
            match self.seen.get(&t.id) {
                None => {
                    out.push(TaskChange::Appeared(t.clone()));
                    if t.is_terminal() {
                        out.push(TaskChange::Finished(t.clone()));
                    }
                }
                Some(prev) if *prev != key => {
                    if t.is_terminal() {
                        out.push(TaskChange::Finished(t.clone()));
                    } else {
                        out.push(TaskChange::Updated(t.clone()));
                    }
                }
                Some(_) => {}
            }
            next.insert(t.id.clone(), key);
        }
        for id in self.seen.keys() {
            if !next.contains_key(id) {
                out.push(TaskChange::Removed(id.clone()));
            }
        }
        self.seen = next;
        out
    }
}

// ---------------------------------------------------------------- instances and projects

/// A registered Pando instance (`/instances`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct InstanceInfo {
    /// Instance id.
    pub instance_id: String,
    /// Working directory.
    pub path: String,
    /// Process id.
    pub pid: i64,
    /// Web port, 0 when none.
    pub web_port: i64,
    /// Mode.
    pub mode: String,
    /// Parent instance.
    pub parent_instance_id: String,
    /// Primary of its project.
    pub is_primary: bool,
}

/// A project (`/projects`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Project {
    /// Project id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Path.
    pub path: String,
    /// Status.
    pub status: String,
    /// Initialised.
    pub initialized: bool,
    /// Launched by another application.
    pub external: bool,
    /// Running delegations.
    pub delegations: i64,
    /// Web state.
    pub web_state: Option<String>,
    /// Web port of its child.
    pub web_port: Option<i64>,
    /// Web URL.
    pub web_url: Option<String>,
}

/// An event of `/projects/events`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectEvent {
    /// `connected|switched|status_changed|init_required|delegation_changed|web_started|web_stopped|web_error`.
    pub event: String,
    /// Project id, when the payload has one.
    pub project_id: Option<String>,
    /// Status, when present.
    pub status: Option<String>,
    /// Raw payload.
    pub data: Value,
}

impl ProjectEvent {
    /// Decodes a frame (tolerant: non-JSON data becomes `Value::Null`).
    pub fn from_sse(ev: &SseEvent) -> Self {
        let data: Value = serde_json::from_str(&ev.data).unwrap_or(Value::Null);
        let s = |k: &str| data.get(k).and_then(Value::as_str).map(str::to_owned);
        Self {
            event: ev.event.clone(),
            project_id: s("project_id"),
            status: s("status"),
            data,
        }
    }
}

// ---------------------------------------------------------------- models, commands, misc

/// A model of `GET /models` (unknown fields ignored).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelInfo {
    /// Id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Provider.
    pub provider: String,
    /// Provider account.
    pub account_id: String,
    /// Badges.
    pub badges: Vec<String>,
    /// Can reason.
    pub can_reason: bool,
    /// Context window.
    pub context_window: Option<i64>,
}

/// `GET /models`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelList {
    /// Models (`auto` first when enabled).
    pub models: Vec<ModelInfo>,
    /// Per-account errors.
    pub errors: std::collections::BTreeMap<String, String>,
    /// Auto routing on.
    pub auto_selected: bool,
}

/// `PUT /models/active` answer.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ActiveModel {
    /// Model.
    pub model: String,
    /// `session|config`.
    pub scope: String,
}

/// A slash command (`GET /commands`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CommandInfo {
    /// Name without slash.
    pub name: String,
    /// Description.
    pub description: String,
    /// Takes arguments.
    pub accepts_args: bool,
}

/// `GET /version`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct VersionInfo {
    /// Version.
    pub version: String,
    /// Latest known.
    pub latest: Option<String>,
    /// Update available.
    pub update_available: bool,
}

/// `GET /project`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ProjectInfo {
    /// Working directory.
    pub cwd: String,
    /// Version.
    pub version: String,
}

fn wrapped<T: serde::de::DeserializeOwned + Default>(v: &Value, key: &str) -> T {
    v.get(key)
        .and_then(|x| serde_json::from_value(x.clone()).ok())
        .unwrap_or_default()
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl PandoClient {
    /// `GET /orchestrator/tasks` (poll every 1-2 s; there is no push stream).
    pub async fn list_tasks(&self) -> Result<TaskList> {
        self.get_json("/api/v1/orchestrator/tasks").await
    }

    /// `GET /orchestrator/tasks/{id}`.
    pub async fn get_task(&self, id: &str) -> Result<TaskResponse> {
        self.get_json(&format!("/api/v1/orchestrator/tasks/{id}"))
            .await
    }

    /// `POST /orchestrator/tasks`.
    pub async fn create_task(&self, task: &CreateTask) -> Result<TaskResponse> {
        self.post_json("/api/v1/orchestrator/tasks", task).await
    }

    /// `POST /orchestrator/tasks/{id}/cancel` (409 if already terminal).
    pub async fn cancel_task(&self, id: &str) -> Result<Value> {
        self.post_json(
            &format!("/api/v1/orchestrator/tasks/{id}/cancel"),
            &serde_json::json!({}),
        )
        .await
    }

    /// `DELETE /orchestrator/tasks/{id}` (204).
    pub async fn delete_task(&self, id: &str) -> Result<()> {
        self.send(self.request(
            reqwest::Method::DELETE,
            &format!("/api/v1/orchestrator/tasks/{id}"),
        ))
        .await?;
        Ok(())
    }

    /// `GET /orchestrator/delegation/metrics`.
    pub async fn delegation_metrics(&self) -> Result<DelegationMetrics> {
        self.get_json("/api/v1/orchestrator/delegation/metrics")
            .await
    }

    /// `GET /instances`.
    pub async fn list_instances(&self) -> Result<Vec<InstanceInfo>> {
        let v: Value = self.get_json("/api/v1/instances").await?;
        Ok(wrapped(&v, "instances"))
    }

    /// `GET /projects`.
    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let v: Value = self.get_json("/api/v1/projects").await?;
        Ok(wrapped(&v, "projects"))
    }

    /// `GET /projects/{id}`.
    pub async fn get_project(&self, id: &str) -> Result<Project> {
        let v: Value = self.get_json(&format!("/api/v1/projects/{id}")).await?;
        Ok(wrapped(&v, "project"))
    }

    /// `POST /projects/{id}/web/open`: starts the project child server.
    pub async fn open_project_web(&self, id: &str) -> Result<Value> {
        self.post_json(
            &format!("/api/v1/projects/{id}/web/open"),
            &serde_json::json!({}),
        )
        .await
    }

    /// `POST /projects/{id}/web/close`.
    pub async fn close_project_web(&self, id: &str) -> Result<Value> {
        self.post_json(
            &format!("/api/v1/projects/{id}/web/close"),
            &serde_json::json!({}),
        )
        .await
    }

    /// The proxy router: a client whose every call goes to the project child through this
    /// server (`/api/v1/projects/{id}/web/api/v1/...`). Credentials are shared; the proxy
    /// swaps in the child's token.
    pub fn project_client(&self, project_id: &str) -> PandoClient {
        self.with_path_prefix(&format!("/api/v1/projects/{project_id}/web"))
    }

    /// `GET /projects/events` as a self-reconnecting stream.
    pub fn project_events(&self, config: EventStreamConfig) -> EventStream<ProjectEvent> {
        EventStream::spawn(self.clone(), "/api/v1/projects/events", config, |e| {
            Some(ProjectEvent::from_sse(e))
        })
    }

    /// `GET /models`.
    pub async fn list_models(&self) -> Result<ModelList> {
        self.get_json("/api/v1/models").await
    }

    /// `PUT /models/active`. Without a session this changes the **global** coder model.
    pub async fn set_active_model(
        &self,
        model: &str,
        session_id: Option<&str>,
    ) -> Result<ActiveModel> {
        let mut body = serde_json::json!({ "model": model });
        if let Some(s) = session_id {
            body["sessionId"] = s.into();
        }
        self.send_json(
            self.request(reqwest::Method::PUT, "/api/v1/models/active")
                .json(&body),
        )
        .await
    }

    /// `GET /commands`.
    pub async fn list_commands(&self) -> Result<Vec<CommandInfo>> {
        self.get_json("/api/v1/commands").await
    }

    /// `GET /version`.
    pub async fn version(&self) -> Result<VersionInfo> {
        self.get_json("/api/v1/version").await
    }

    /// `GET /project`.
    pub async fn project_info(&self) -> Result<ProjectInfo> {
        self.get_json("/api/v1/project").await
    }

    /// `GET /files/search?q=` (for @-mentions).
    pub async fn files_search(&self, query: &str) -> Result<Value> {
        self.get_json(&format!("/api/v1/files/search?q={}", enc(query)))
            .await
    }
}
