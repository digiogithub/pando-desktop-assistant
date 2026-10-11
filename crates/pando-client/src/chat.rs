//! Sessions, history and the chat SSE protocol (`docs/research/pando-api.md` sections 2.2, 3).
//!
//! All types are tolerant: unknown fields are ignored, missing ones default, and unknown SSE
//! events or message parts are preserved (`Unknown{raw}` / `Part::Other`).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::client::{PandoClient, SseStream};
use crate::error::Result;
use crate::sse::SseEvent;

fn null_default<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

// ---------------------------------------------------------------- sessions and history

/// A session (PascalCase on the wire; timestamps are Unix seconds).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Session {
    /// Session id.
    #[serde(rename = "ID")]
    pub id: String,
    /// Parent session (delegations), empty when none.
    #[serde(rename = "ParentSessionID")]
    pub parent_session_id: String,
    /// Title.
    #[serde(rename = "Title")]
    pub title: String,
    /// Number of messages.
    #[serde(rename = "MessageCount")]
    pub message_count: i64,
    /// Prompt tokens.
    #[serde(rename = "PromptTokens")]
    pub prompt_tokens: i64,
    /// Completion tokens.
    #[serde(rename = "CompletionTokens")]
    pub completion_tokens: i64,
    /// Cost in USD.
    #[serde(rename = "Cost")]
    pub cost: f64,
    /// Creation time (Unix seconds).
    #[serde(rename = "CreatedAt")]
    pub created_at: i64,
    /// Last update (Unix seconds).
    #[serde(rename = "UpdatedAt")]
    pub updated_at: i64,
}

/// A session as listed by `GET /sessions` (adds run state).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SessionSummary {
    /// The session fields.
    #[serde(flatten)]
    pub session: Session,
    /// A run is in progress.
    pub is_running: bool,
    /// Run counter: grows by one per run, including resurrected ones.
    pub run_seq: i64,
    /// Preview of the first prompt.
    pub prompt_preview: Option<String>,
}

/// `GET /sessions` page.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SessionList {
    /// Newest first.
    pub sessions: Vec<SessionSummary>,
    /// Total sessions.
    pub total: i64,
    /// Page size.
    pub limit: i64,
    /// Offset.
    pub offset: i64,
    /// More pages exist.
    pub has_more: bool,
}

/// `GET /sessions/{id}`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SessionDetail {
    /// The session.
    pub session: Session,
    /// History, oldest first.
    #[serde(deserialize_with = "null_default")]
    pub messages: Vec<Message>,
    /// A run is in progress.
    pub is_running: bool,
    /// Run counter.
    pub run_seq: i64,
}

/// A history message.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Message {
    /// Message id.
    #[serde(rename = "ID")]
    pub id: String,
    /// `user|assistant|system|tool`.
    #[serde(rename = "Role")]
    pub role: String,
    /// Owning session.
    #[serde(rename = "SessionID")]
    pub session_id: String,
    /// Content parts.
    #[serde(rename = "Parts", deserialize_with = "null_default")]
    pub parts: Vec<Part>,
    /// Model that produced it.
    #[serde(rename = "Model")]
    pub model: String,
    /// Creation time (Unix seconds).
    #[serde(rename = "CreatedAt")]
    pub created_at: i64,
}

/// A tool call part (input is a raw JSON string).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ToolCallPart {
    /// Call id.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Raw JSON arguments.
    pub input: String,
    /// The call finished streaming.
    pub finished: bool,
}

/// A tool result part (lives in `tool` messages).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ToolResultPart {
    /// Id of the call it answers.
    pub tool_call_id: String,
    /// Tool name.
    pub name: String,
    /// Output text.
    pub content: String,
    /// Raw JSON metadata string.
    pub metadata: String,
    /// The tool failed.
    pub is_error: bool,
}

/// One content part of a message. Discriminated by keys, since Pando writes them untagged.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// Plain text.
    Text(String),
    /// Reasoning text.
    Thinking(String),
    /// A tool call.
    ToolCall(ToolCallPart),
    /// A tool result.
    ToolResult(ToolResultPart),
    /// End of a turn: `end_turn|max_tokens|tool_use|canceled|error|permission_denied|unknown`.
    Finish {
        /// Reason.
        reason: String,
        /// Unix seconds.
        time: i64,
    },
    /// An image URL.
    ImageUrl(String),
    /// A binary attachment (base64 data).
    Binary {
        /// File name.
        path: String,
        /// MIME type.
        mime_type: String,
    },
    /// Anything else, preserved.
    Other(Value),
}

impl<'de> Deserialize<'de> for Part {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        Ok(Self::from_value(Value::deserialize(d)?))
    }
}

impl Part {
    /// Classifies a raw part object.
    pub fn from_value(v: Value) -> Self {
        let Some(o) = v.as_object() else {
            return Self::Other(v);
        };
        let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_owned);
        if o.contains_key("tool_call_id") {
            if let Ok(r) = serde_json::from_value(v.clone()) {
                return Self::ToolResult(r);
            }
        } else if o.contains_key("id") && o.contains_key("name") && o.contains_key("input") {
            if let Ok(c) = serde_json::from_value(v.clone()) {
                return Self::ToolCall(c);
            }
        } else if let Some(t) = s("text") {
            return Self::Text(t);
        } else if let Some(t) = s("thinking") {
            return Self::Thinking(t);
        } else if let Some(reason) = s("reason") {
            return Self::Finish {
                reason,
                time: o.get("time").and_then(Value::as_i64).unwrap_or(0),
            };
        } else if let Some(u) = s("url") {
            return Self::ImageUrl(u);
        } else if o.contains_key("Path") && o.contains_key("Data") {
            return Self::Binary {
                path: s("Path").unwrap_or_default(),
                mime_type: s("MIMEType").unwrap_or_default(),
            };
        }
        Self::Other(v)
    }
}

/// A rendered item of a folded message.
#[derive(Debug, Clone, PartialEq)]
pub enum FoldedItem {
    /// Text.
    Text(String),
    /// Reasoning.
    Thinking(String),
    /// A tool call with its result, when one exists in the history.
    Tool {
        /// The call.
        call: ToolCallPart,
        /// Its result.
        result: Option<ToolResultPart>,
    },
    /// Image URL.
    Image(String),
    /// Finish marker.
    Finish(String),
}

/// A message after folding tool results into their calls.
#[derive(Debug, Clone, PartialEq)]
pub struct FoldedMessage {
    /// Message id.
    pub id: String,
    /// `user|assistant|system`.
    pub role: String,
    /// Model.
    pub model: String,
    /// Creation time.
    pub created_at: i64,
    /// Items in order.
    pub items: Vec<FoldedItem>,
}

/// Folds `tool` messages into the `ToolCall` they answer and drops them (what the WebUI does).
pub fn fold_messages(messages: &[Message]) -> Vec<FoldedMessage> {
    let mut results = std::collections::HashMap::new();
    for m in messages {
        for p in &m.parts {
            if let Part::ToolResult(r) = p {
                results.insert(r.tool_call_id.clone(), r.clone());
            }
        }
    }
    let mut out = Vec::new();
    for m in messages.iter().filter(|m| m.role != "tool") {
        let items = m
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Text(t) => Some(FoldedItem::Text(t.clone())),
                Part::Thinking(t) => Some(FoldedItem::Thinking(t.clone())),
                Part::ToolCall(c) => Some(FoldedItem::Tool {
                    result: results.get(&c.id).cloned(),
                    call: c.clone(),
                }),
                Part::ImageUrl(u) => Some(FoldedItem::Image(u.clone())),
                Part::Finish { reason, .. } => Some(FoldedItem::Finish(reason.clone())),
                Part::ToolResult(_) | Part::Binary { .. } | Part::Other(_) => None,
            })
            .collect();
        out.push(FoldedMessage {
            id: m.id.clone(),
            role: m.role.clone(),
            model: m.model.clone(),
            created_at: m.created_at,
            items,
        });
    }
    out
}

// ---------------------------------------------------------------- chat events

/// Permission request (also returned by `/pending`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PermissionRequest {
    /// Request id.
    pub id: String,
    /// Session.
    pub session_id: String,
    /// Tool asking.
    pub tool_name: String,
    /// Human description.
    pub description: String,
    /// Action (`execute_unsandboxed` means a sandbox escalation).
    pub action: String,
    /// Path concerned.
    pub path: String,
    /// Raw tool parameters.
    pub params: Value,
    /// Model's justification.
    pub justification: Option<String>,
    /// Key for "allow for session" grants.
    pub grant_key: Option<String>,
    /// Never auto-approve this one (fail-closed).
    pub require_explicit_approval: bool,
    /// Never auto-approve this one (fail-closed).
    pub never_auto_approve: bool,
}

/// One option of a question.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct QuestionOption {
    /// Label (also the answer value).
    pub label: String,
    /// Description.
    pub description: String,
}

/// One question.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Question {
    /// Question id.
    pub id: String,
    /// Short header.
    pub header: String,
    /// Text.
    pub question: String,
    /// Several options may be selected.
    pub multi_select: bool,
    /// Options.
    #[serde(deserialize_with = "null_default")]
    pub options: Vec<QuestionOption>,
}

/// `AskUserQuestion` request.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct QuestionRequest {
    /// Request id.
    pub id: String,
    /// Session.
    pub session_id: String,
    /// Questions.
    #[serde(deserialize_with = "null_default")]
    pub questions: Vec<Question>,
}

/// A location touched by a tool.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Location {
    /// File path.
    pub path: String,
}

/// `tool_call` / `tool_call_update` payload.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ToolCallEvent {
    /// Call id.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// `execute|edit|read|search|fetch|think|switch_mode|other`.
    pub kind: String,
    /// Display title.
    pub title: String,
    /// `pending|in_progress`.
    pub status: String,
    /// Raw JSON arguments.
    pub input: String,
    /// Touched locations.
    #[serde(deserialize_with = "null_default")]
    pub locations: Vec<Location>,
}

/// `tool_result` payload.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ToolResultEvent {
    /// Id of the call.
    pub tool_call_id: String,
    /// Tool name.
    pub name: String,
    /// Kind.
    pub kind: String,
    /// Title.
    pub title: String,
    /// `completed|failed`.
    pub status: String,
    /// Output text.
    pub content: String,
    /// Failure flag.
    pub is_error: bool,
    /// Touched locations.
    #[serde(deserialize_with = "null_default")]
    pub locations: Vec<Location>,
    /// Raw output with optional metadata.
    pub raw_output: Value,
    /// Terminal info (`terminal_id`, `exit_code`).
    pub terminal: Option<Value>,
    /// Diff (edit/write).
    pub diff: Option<Value>,
    /// `data:` URLs of images.
    pub images: Option<Vec<String>>,
}

/// Plan entry from TodoWrite.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PlanEntry {
    /// Title.
    pub title: String,
    /// `pending|in_progress|completed`.
    pub status: String,
    /// Present-tense form.
    pub active_form: Option<String>,
}

/// Todo item.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Todo {
    /// Text.
    pub content: String,
    /// Status.
    pub status: String,
    /// Priority.
    pub priority: String,
}

/// `token_usage` payload.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TokenUsage {
    /// Session.
    pub session_id: String,
    /// Prompt tokens.
    pub prompt_tokens: i64,
    /// Completion tokens.
    pub completion_tokens: i64,
    /// Context window.
    pub context_window: i64,
    /// Values are estimates (breakdown not meaningful).
    pub estimated: bool,
    /// Cache read tokens.
    pub cache_read_tokens: i64,
    /// Cache creation tokens.
    pub cache_creation_tokens: i64,
    /// Reasoning tokens.
    pub reasoning_tokens: i64,
    /// Cost in USD.
    pub cost: f64,
}

/// `system_message` payload.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SystemMessage {
    /// Session.
    pub session_id: String,
    /// Text.
    pub text: String,
    /// Auto-routing info.
    pub routing: Option<Value>,
    /// Context filter info.
    pub context_filter: Option<Value>,
}

/// Payload of steering, conclusion and resurrection notes.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Note {
    /// Session.
    pub session_id: String,
    /// Message.
    pub message: String,
}

/// A goal (`/goal`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Goal {
    /// Objective.
    pub objective: String,
    /// Status.
    pub status: String,
    /// Iteration.
    pub iteration: i64,
    /// Max iterations.
    pub max_iterations: i64,
    /// Progress note.
    pub progress: Option<Value>,
    /// Next step.
    pub next_step: Option<String>,
    /// Start time.
    pub started_at: Option<String>,
    /// Completion time.
    pub completed_at: Option<String>,
}

/// One event of the chat stream.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    /// First event: the (possibly new) session and whether a run is active.
    Session {
        /// Session id (absent on the closing `running:false` event of slash streams).
        session_id: String,
        /// A run is active.
        running: bool,
    },
    /// Run counter of this session.
    Run {
        /// Session id.
        session_id: String,
        /// Run sequence.
        run_seq: i64,
    },
    /// Permission needed.
    PermissionRequest(PermissionRequest),
    /// Question for the user.
    QuestionRequest(QuestionRequest),
    /// Reasoning delta.
    ThinkingDelta(String),
    /// Answer text delta: accumulate until `Done`.
    ContentDelta(String),
    /// Tool started.
    ToolCall(ToolCallEvent),
    /// Tool input finished streaming.
    ToolCallUpdate(ToolCallEvent),
    /// Tool finished.
    ToolResult(ToolResultEvent),
    /// TodoWrite plan.
    PlanUpdate(Vec<PlanEntry>),
    /// Todo list.
    TodosUpdate(Vec<Todo>),
    /// Token accounting.
    TokenUsage(TokenUsage),
    /// System/routing note.
    SystemMessage(SystemMessage),
    /// A steering prompt was queued.
    SteeringQueued(Note),
    /// A steering prompt was injected: close the bubble, open a new one.
    SteeringInjected(Note),
    /// A delegation conclusion was queued (human note only).
    ConclusionQueued(Note),
    /// A delegation conclusion was injected.
    ConclusionInjected(Note),
    /// First event of a system-initiated (resurrected) run.
    Resurrected(Note),
    /// Goal state.
    GoalStatus(Option<Goal>),
    /// Run error.
    Error(String),
    /// Run finished. A body that closes without it may still be running: reattach.
    Done,
    /// An event this client does not know (or could not decode).
    Unknown {
        /// Event name.
        event: String,
        /// Raw data.
        raw: String,
    },
}

fn de<T: DeserializeOwned>(data: &str) -> Option<T> {
    serde_json::from_str(data).ok()
}

impl ChatEvent {
    /// Decodes one SSE frame. Never fails: undecodable frames become [`ChatEvent::Unknown`].
    pub fn from_sse(ev: &SseEvent) -> Self {
        Self::try_decode(ev).unwrap_or_else(|| Self::Unknown {
            event: ev.event.clone(),
            raw: ev.data.clone(),
        })
    }

    fn try_decode(ev: &SseEvent) -> Option<Self> {
        let d = ev.data.as_str();
        Some(match ev.event.as_str() {
            "session" => {
                #[derive(Deserialize, Default)]
                #[serde(default, rename_all = "camelCase")]
                struct B {
                    session_id: String,
                    running: bool,
                }
                let b: B = de(d)?;
                Self::Session {
                    session_id: b.session_id,
                    running: b.running,
                }
            }
            "run" => {
                #[derive(Deserialize, Default)]
                #[serde(default, rename_all = "camelCase")]
                struct B {
                    session_id: String,
                    run_seq: i64,
                }
                let b: B = de(d)?;
                Self::Run {
                    session_id: b.session_id,
                    run_seq: b.run_seq,
                }
            }
            "permission_request" => Self::PermissionRequest(de(d)?),
            "question_request" => Self::QuestionRequest(de(d)?),
            "thinking_delta" | "content_delta" => {
                #[derive(Deserialize, Default)]
                #[serde(default)]
                struct B {
                    text: String,
                }
                let b: B = de(d)?;
                if ev.event == "content_delta" {
                    Self::ContentDelta(b.text)
                } else {
                    Self::ThinkingDelta(b.text)
                }
            }
            "tool_call" => Self::ToolCall(de(d)?),
            "tool_call_update" => Self::ToolCallUpdate(de(d)?),
            "tool_result" => Self::ToolResult(de(d)?),
            "plan_update" => {
                #[derive(Deserialize, Default)]
                #[serde(default)]
                struct B {
                    #[serde(deserialize_with = "null_default")]
                    entries: Vec<PlanEntry>,
                }
                Self::PlanUpdate(de::<B>(d)?.entries)
            }
            "todos_update" => {
                #[derive(Deserialize, Default)]
                #[serde(default)]
                struct B {
                    #[serde(deserialize_with = "null_default")]
                    todos: Vec<Todo>,
                }
                Self::TodosUpdate(de::<B>(d)?.todos)
            }
            "token_usage" => Self::TokenUsage(de(d)?),
            "system_message" => Self::SystemMessage(de(d)?),
            "steering_queued" => Self::SteeringQueued(de(d)?),
            "steering_injected" => Self::SteeringInjected(de(d)?),
            "conclusion_queued" => Self::ConclusionQueued(de(d)?),
            "conclusion_injected" => Self::ConclusionInjected(de(d)?),
            "resurrected" => Self::Resurrected(de(d)?),
            "goal_status" => {
                #[derive(Deserialize, Default)]
                #[serde(default)]
                struct B {
                    goal: Option<Goal>,
                }
                Self::GoalStatus(de::<B>(d)?.goal)
            }
            "error" => {
                #[derive(Deserialize, Default)]
                #[serde(default)]
                struct B {
                    error: String,
                }
                Self::Error(de::<B>(d)?.error)
            }
            "done" => Self::Done,
            _ => return None,
        })
    }
}

/// A chat/attach stream decoded into [`ChatEvent`]s.
#[derive(Debug)]
pub struct ChatStream {
    inner: SseStream,
}

impl ChatStream {
    /// The next event, `Ok(None)` when the body closed.
    pub async fn next(&mut self) -> Result<Option<ChatEvent>> {
        Ok(self
            .inner
            .next_event()
            .await?
            .map(|e| ChatEvent::from_sse(&e)))
    }
}

// ---------------------------------------------------------------- calls

/// `GET /sessions/{id}/pending`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Pending {
    /// Pending permission requests.
    #[serde(deserialize_with = "null_default")]
    pub permissions: Vec<PermissionRequest>,
    /// Pending questions.
    #[serde(deserialize_with = "null_default")]
    pub questions: Vec<QuestionRequest>,
    /// A run is active.
    pub running: bool,
    /// Run counter.
    pub run_seq: i64,
}

/// `GET /sessions/{id}/model`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionModel {
    /// Session.
    pub session_id: String,
    /// Effective model.
    pub model: String,
    /// The session overrides the global model.
    pub r#override: bool,
    /// Auto routing is active.
    pub auto_selected: bool,
}

/// Answer to a permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    /// Allow once.
    Allow,
    /// Allow for the session.
    AllowSession,
    /// Deny.
    Deny,
}

/// Answer to one question.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAnswer {
    /// Question id.
    pub question_id: String,
    /// Selected labels.
    pub selected: Vec<String>,
    /// Free text.
    pub other_text: String,
}

fn sess(id: &str, tail: &str) -> String {
    format!("/api/v1/sessions/{id}{tail}")
}

impl PandoClient {
    /// `GET /sessions?limit=&offset=`.
    pub async fn list_sessions(&self, limit: u32, offset: u32) -> Result<SessionList> {
        self.get_json(&format!("/api/v1/sessions?limit={limit}&offset={offset}"))
            .await
    }

    /// `GET /sessions/{id}`.
    pub async fn get_session(&self, id: &str) -> Result<SessionDetail> {
        self.get_json(&sess(id, "")).await
    }

    /// `PATCH /sessions/{id}` with a new title.
    pub async fn rename_session(&self, id: &str, title: &str) -> Result<Value> {
        self.send_json(
            self.request(reqwest::Method::PATCH, &sess(id, ""))
                .json(&serde_json::json!({ "title": title })),
        )
        .await
    }

    /// `DELETE /sessions/{id}`.
    pub async fn delete_session(&self, id: &str) -> Result<Value> {
        self.send_json(self.request(reqwest::Method::DELETE, &sess(id, "")))
            .await
    }

    /// `POST /chat/stream`. An empty/`None` session creates one (its id arrives in `Session`).
    pub async fn chat_stream(
        &self,
        session_id: Option<&str>,
        prompt: &str,
        model: Option<&str>,
    ) -> Result<ChatStream> {
        let mut body = serde_json::json!({ "prompt": prompt });
        if let Some(s) = session_id.filter(|s| !s.is_empty()) {
            body["sessionId"] = s.into();
        }
        if let Some(m) = model.filter(|m| !m.is_empty()) {
            body["model"] = m.into();
        }
        let inner = self.open_sse("/api/v1/chat/stream", Some(&body)).await?;
        Ok(ChatStream { inner })
    }

    /// `GET /sessions/{id}/stream`: replay of the current run, then live events.
    pub async fn attach_stream(&self, id: &str) -> Result<ChatStream> {
        let inner = self.open_sse::<()>(&sess(id, "/stream"), None).await?;
        Ok(ChatStream { inner })
    }

    /// `POST /sessions/{id}/steer` (409 `not_busy` when no run is active).
    pub async fn steer(&self, id: &str, prompt: &str) -> Result<Value> {
        self.post_json(
            &sess(id, "/steer"),
            &serde_json::json!({ "prompt": prompt }),
        )
        .await
    }

    /// `POST /sessions/{id}/cancel`.
    pub async fn cancel(&self, id: &str) -> Result<Value> {
        self.post_json(&sess(id, "/cancel"), &serde_json::json!({}))
            .await
    }

    /// `GET /sessions/{id}/pending`: pending approvals plus the run heartbeat.
    pub async fn pending(&self, id: &str) -> Result<Pending> {
        self.get_json(&sess(id, "/pending")).await
    }

    /// `GET /sessions/{id}/auto-approve`.
    pub async fn auto_approve(&self, id: &str) -> Result<bool> {
        let v: Value = self.get_json(&sess(id, "/auto-approve")).await?;
        Ok(v.get("enabled").and_then(Value::as_bool).unwrap_or(false))
    }

    /// `POST /sessions/{id}/auto-approve`.
    pub async fn set_auto_approve(&self, id: &str, enabled: bool) -> Result<bool> {
        let v: Value = self
            .post_json(
                &sess(id, "/auto-approve"),
                &serde_json::json!({ "enabled": enabled }),
            )
            .await?;
        Ok(v.get("enabled").and_then(Value::as_bool).unwrap_or(false))
    }

    /// `GET /sessions/{id}/model`.
    pub async fn session_model(&self, id: &str) -> Result<SessionModel> {
        self.get_json(&sess(id, "/model")).await
    }

    /// `GET /sessions/{id}/goal`.
    pub async fn goal(&self, id: &str) -> Result<Option<Goal>> {
        let v: Value = self.get_json(&sess(id, "/goal")).await?;
        Ok(v.get("goal")
            .filter(|g| !g.is_null())
            .and_then(|g| serde_json::from_value(g.clone()).ok()))
    }

    /// `POST /sessions/{id}/goal/cancel` (404 when no goal is active).
    pub async fn goal_cancel(&self, id: &str) -> Result<Value> {
        self.post_json(&sess(id, "/goal/cancel"), &serde_json::json!({}))
            .await
    }

    /// `POST /permissions/respond`.
    pub async fn permissions_respond(
        &self,
        id: &str,
        session_id: &str,
        action: PermissionAction,
    ) -> Result<Value> {
        self.post_json(
            "/api/v1/permissions/respond",
            &serde_json::json!({ "id": id, "sessionId": session_id, "action": action }),
        )
        .await
    }

    /// `POST /questions/respond`.
    pub async fn questions_respond(
        &self,
        id: &str,
        session_id: &str,
        answers: &[QuestionAnswer],
        cancelled: bool,
    ) -> Result<Value> {
        self.post_json(
            "/api/v1/questions/respond",
            &serde_json::json!({
                "id": id, "sessionId": session_id, "answers": answers, "cancelled": cancelled
            }),
        )
        .await
    }
}
