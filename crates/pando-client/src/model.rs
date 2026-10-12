//! `ChatModel`: the GPUI-free state behind the chat view (PDA-T-0063, 0065, 0073, 0077, 0078).
//!
//! It folds [`ChatEvent`]s (live, replayed or from history) into a transcript the view renders,
//! and owns the side state of a session: approvals ([`Approvals`]), the steering queue, plan,
//! todos, goal and usage.
//!
//! How a view binds to it:
//! - read [`ChatModel::items`] and re-render when [`ChatModel::revision`] changes;
//! - feed it with [`ChatModel::apply`] (own chat stream), [`ChatModel::apply_follow`] (events of
//!   [`crate::follow::SessionFollow`]) and [`ChatModel::load_history`];
//! - guard against late events of a previous session with [`ChatModel::begin_session`]'s
//!   generation and [`ChatModel::apply_gen`];
//! - user actions: [`ChatModel::submit`], [`ChatModel::decide`], [`ChatModel::stop_plan`].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::approvals::{
    Approvals, Clock, Decision, DenyReason, Origin, Refused, RequestBody, Resolution, SystemClock,
};
use crate::chat::{
    ChatEvent, FoldedItem, Goal, Location, PlanEntry, SessionDetail, Todo, ToolCallEvent,
    ToolResultEvent, fold_messages,
};
use crate::conclusion::{DelegatedResult, parse_delegated_results};
use crate::follow::{FollowEvent, seen_for};
use crate::steering::{SendPlan, SteeringQueue, StopPlan, stop_plan};
use crate::usage::UsageState;

/// Stable id of a transcript item (use it as the list key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemId(pub u64);

/// State of a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    /// Announced, input still streaming.
    Pending,
    /// Running.
    Running,
    /// Finished fine.
    Completed,
    /// Finished with an error.
    Failed,
}

/// The result of a tool call.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolOutcome {
    /// Output text.
    pub content: String,
    /// The tool reported an error.
    pub is_error: bool,
    /// Edit/write diff.
    pub diff: Option<Value>,
    /// Terminal info (`terminal_id`, `exit_code`).
    pub terminal: Option<Value>,
    /// `data:` URLs of images.
    pub images: Vec<String>,
    /// Raw output (with metadata).
    pub raw_output: Value,
}

/// A tool call row.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolItem {
    /// Call id.
    pub call_id: String,
    /// Tool name.
    pub name: String,
    /// `execute|edit|read|search|fetch|think|switch_mode|other`.
    pub kind: String,
    /// Display title.
    pub title: String,
    /// Status.
    pub status: ToolStatus,
    /// Raw JSON arguments.
    pub input: String,
    /// Touched files.
    pub locations: Vec<String>,
    /// Result, once there is one.
    pub outcome: Option<ToolOutcome>,
    /// How long it ran (live runs only; history has no timing).
    pub duration: Option<Duration>,
    started: Option<Duration>,
}

/// Kinds of notices (one line of context between messages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoticeKind {
    /// Auto-routing decision.
    Routing,
    /// Other system message (context enrichment, ...).
    System,
    /// A system-initiated run started (resurrected).
    Resurrected,
    /// A delegation conclusion was queued.
    ConclusionQueued,
    /// A delegation conclusion was injected.
    ConclusionInjected,
    /// The run was cancelled.
    Cancelled,
    /// The answer hit the token limit.
    MaxTokens,
    /// A tool was refused by the permission system.
    PermissionDenied,
    /// The run failed.
    Error,
}

/// What a transcript row is.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    /// A user message (`steered`: it was injected into a running run).
    User {
        /// Text.
        text: String,
        /// Injected via steering.
        steered: bool,
    },
    /// Assistant text (a bubble segment).
    Assistant {
        /// Text so far.
        text: String,
        /// More deltas may follow.
        streaming: bool,
    },
    /// Model reasoning.
    Thinking {
        /// Text so far.
        text: String,
        /// More deltas may follow.
        streaming: bool,
    },
    /// A tool call with its result folded in.
    Tool(ToolItem),
    /// An approval card; the data lives in [`ChatModel::approval_card`].
    Approval {
        /// Request id.
        request_id: String,
    },
    /// A notice line.
    Notice {
        /// Kind.
        kind: NoticeKind,
        /// Text.
        text: String,
    },
    /// A delegation result (`[delegated-result ...]` block).
    Conclusion(Box<DelegatedResult>),
    /// An image.
    Image {
        /// URL or `data:` URL.
        url: String,
    },
    /// A binary attachment.
    Attachment {
        /// File name.
        path: String,
        /// MIME type.
        mime_type: String,
    },
    /// A part or event this client does not know, shown as JSON.
    Json {
        /// Short label.
        label: String,
        /// Payload.
        value: Value,
    },
}

/// A transcript row.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Stable id.
    pub id: ItemId,
    /// Content.
    pub kind: ItemKind,
}

/// What an approval card shows.
#[derive(Debug, Clone)]
pub struct ApprovalCard<'a> {
    /// The request.
    pub request: &'a RequestBody,
    /// `None` while pending.
    pub resolution: Option<&'a Resolution>,
    /// Still waiting for the user.
    pub pending: bool,
    /// Only a deliberate click may approve it.
    pub explicit_only: bool,
}

/// Goal banner data.
#[derive(Debug, Clone, PartialEq)]
pub struct GoalBanner {
    /// Objective.
    pub objective: String,
    /// `running|completed|failed|cancelled|blocked|timeout|stalled`.
    pub status: String,
    /// Iteration (1-based as Pando reports it).
    pub iteration: i64,
    /// Iteration cap.
    pub max_iterations: i64,
    /// Progress note.
    pub progress: Option<String>,
    /// Next step.
    pub next_step: Option<String>,
    /// The goal is still running, so Cancel is offered.
    pub can_cancel: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    None,
    Text(usize),
    Thinking(usize),
}

/// The chat state of one session.
pub struct ChatModel {
    clock: Arc<dyn Clock>,
    generation: u64,
    revision: u64,
    next_id: u64,
    session_id: Option<String>,
    title: String,
    items: Vec<Item>,
    open: Open,
    tools: HashMap<String, usize>,
    run_active: bool,
    run_seq: i64,
    run_start: Option<usize>,
    /// A replay is about to start: `Some(run was active)` until its `Run` event says which run.
    pending_reset: Option<bool>,
    seen_run_seq: i64,
    approvals: Approvals,
    steering: SteeringQueue,
    orphaned: Vec<String>,
    plan: Vec<PlanEntry>,
    todos: Vec<Todo>,
    goal: Option<Goal>,
    usage: UsageState,
}

impl std::fmt::Debug for ChatModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatModel")
            .field("session_id", &self.session_id)
            .field("items", &self.items.len())
            .field("run_active", &self.run_active)
            .finish_non_exhaustive()
    }
}

impl Default for ChatModel {
    fn default() -> Self {
        Self::new(Arc::new(SystemClock::default()))
    }
}

impl ChatModel {
    /// A model using `clock` for tool durations and approval deadlines.
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            generation: 0,
            revision: 0,
            next_id: 0,
            session_id: None,
            title: String::new(),
            items: Vec::new(),
            open: Open::None,
            tools: HashMap::new(),
            run_active: false,
            run_seq: 0,
            run_start: None,
            pending_reset: None,
            seen_run_seq: -1,
            approvals: Approvals::default(),
            steering: SteeringQueue::new(),
            orphaned: Vec::new(),
            plan: Vec::new(),
            todos: Vec::new(),
            goal: None,
            usage: UsageState::default(),
        }
    }

    // ------------------------------------------------------------ session lifecycle

    /// Switches to another session (or a new one when `None`): clears everything, denies pending
    /// approvals of the old session and returns the new generation. Events tagged with an older
    /// generation are dropped by [`ChatModel::apply_gen`].
    pub fn begin_session(&mut self, session_id: Option<&str>) -> u64 {
        self.approvals.deny_all(DenyReason::SessionSwitch);
        let pending_answers = self.approvals.outbox();
        let generation = self.generation + 1;
        let clock = self.clock.clone();
        let carried = if pending_answers.is_empty() {
            Approvals::default()
        } else {
            std::mem::take(&mut self.approvals)
        };
        *self = Self::new(clock);
        self.approvals = carried; // undelivered denials must still reach Pando
        self.generation = generation;
        self.session_id = session_id.map(str::to_owned);
        self.bump();
        generation
    }

    /// The current generation.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Applies `ev` only if `generation` is still current. Returns whether it was applied.
    pub fn apply_gen(&mut self, generation: u64, ev: &ChatEvent) -> bool {
        if generation != self.generation {
            return false;
        }
        self.apply(ev);
        true
    }

    /// Increments on every change: the view re-renders when it differs from the last seen value.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn bump(&mut self) {
        self.revision += 1;
    }

    fn push(&mut self, kind: ItemKind) -> usize {
        self.next_id += 1;
        self.items.push(Item {
            id: ItemId(self.next_id),
            kind,
        });
        self.items.len() - 1
    }

    // ------------------------------------------------------------ reading

    /// The transcript.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Session id, once known.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Session title (from history).
    pub fn title(&self) -> &str {
        &self.title
    }

    /// A run is active (the Stop button shows, new messages become steering).
    pub fn is_running(&self) -> bool {
        self.run_active
    }

    /// `seenRunSeq` for [`crate::follow::follow_session`].
    pub fn seen_run_seq(&self) -> i64 {
        self.seen_run_seq
    }

    /// Plan entries (kept after the run ends).
    pub fn plan(&self) -> &[PlanEntry] {
        &self.plan
    }

    /// Todos.
    pub fn todos(&self) -> &[Todo] {
        &self.todos
    }

    /// The goal as last reported.
    pub fn goal(&self) -> Option<&Goal> {
        self.goal.as_ref()
    }

    /// Goal banner data; `None` when there is no goal.
    pub fn goal_banner(&self) -> Option<GoalBanner> {
        let g = self.goal.as_ref()?;
        let text = |v: &Value| match v {
            Value::Null => None,
            Value::String(s) => Some(s.clone()).filter(|s| !s.is_empty()),
            other => Some(other.to_string()),
        };
        Some(GoalBanner {
            objective: g.objective.clone(),
            status: g.status.clone(),
            iteration: g.iteration,
            max_iterations: g.max_iterations,
            progress: g.progress.as_ref().and_then(text),
            next_step: g.next_step.clone().filter(|s| !s.is_empty()),
            can_cancel: g.status == "running",
        })
    }

    /// Usage panel data.
    pub fn usage(&self) -> &UsageState {
        &self.usage
    }

    /// Steering chips ("En cola").
    pub fn steering(&self) -> &SteeringQueue {
        &self.steering
    }

    /// Mutable steering queue (report steer outcomes on it).
    pub fn steering_mut(&mut self) -> &mut SteeringQueue {
        &mut self.steering
    }

    /// The approvals ledger.
    pub fn approvals(&self) -> &Approvals {
        &self.approvals
    }

    /// Mutable ledger (to `deliver` the outbox).
    pub fn approvals_mut(&mut self) -> &mut Approvals {
        &mut self.approvals
    }

    /// Card data for an approval row.
    pub fn approval_card(&self, request_id: &str) -> Option<ApprovalCard<'_>> {
        Some(ApprovalCard {
            request: self.approvals.request(request_id)?,
            resolution: self.approvals.resolution(request_id),
            pending: self.approvals.is_pending(request_id),
            explicit_only: self.approvals.is_flagged(request_id),
        })
    }

    // ------------------------------------------------------------ user actions

    /// The user sent `text`: returns what to do. A normal prompt also appends the user message
    /// and starts a run locally (call it before opening `chat_stream`); steering adds a chip.
    pub fn submit(&mut self, text: &str) -> SendPlan {
        let plan = self.steering.submit(text, self.run_active);
        if let SendPlan::Prompt(t) = &plan {
            self.start_local_run(t);
        } else {
            self.bump();
        }
        plan
    }

    /// Appends a user message and marks a run as starting (used by [`ChatModel::submit`] and when
    /// re-sending orphaned steering text).
    pub fn start_local_run(&mut self, text: &str) {
        self.close_open();
        self.push(ItemKind::User {
            text: text.to_owned(),
            steered: false,
        });
        self.run_start = Some(self.items.len());
        self.run_active = true;
        self.bump();
    }

    /// Steering text whose run ended before it was injected: send each as a normal prompt.
    pub fn take_orphaned_steering(&mut self) -> Vec<String> {
        std::mem::take(&mut self.orphaned)
    }

    /// What Stop must do now.
    pub fn stop_plan(&self) -> StopPlan {
        stop_plan(self.run_active, self.goal.as_ref())
    }

    /// Applies a user decision to an approval.
    pub fn decide(
        &mut self,
        request_id: &str,
        decision: Decision,
        origin: Origin,
    ) -> Result<Resolution, Refused> {
        let r = self.approvals.decide(request_id, decision, origin)?;
        self.bump();
        Ok(r)
    }

    /// Denies approvals whose deadline passed; returns their ids. Call when the clock reaches
    /// [`Approvals::next_deadline`]. Then deliver the outbox.
    pub fn expire_approvals(&mut self) -> Vec<String> {
        let ids = self.approvals.expire(self.clock.now());
        if !ids.is_empty() {
            self.bump();
        }
        ids
    }

    /// The panel closed or the app is quitting: deny everything still pending.
    pub fn deny_all_pending(&mut self, reason: DenyReason) -> Vec<String> {
        let ids = self.approvals.deny_all(reason);
        if !ids.is_empty() {
            self.bump();
        }
        ids
    }

    // ------------------------------------------------------------ events

    /// Applies an event of the follower ([`crate::follow::SessionFollow`]).
    pub fn apply_follow(&mut self, ev: &FollowEvent) {
        match ev {
            FollowEvent::ResetBubble => self.reset_live(),
            FollowEvent::Chat(c) => self.apply(c),
            FollowEvent::HistoryReload(d) => self.load_history(d),
            FollowEvent::RunFinished => {}
        }
    }

    /// Drops the in-flight run's rows (a replay re-delivers all of them) and the live state.
    ///
    /// The cut is deferred to the replay's `Run` event: a run newer than the last one seen (a
    /// resurrected run) starts after the rows of the finished run, which are kept.
    pub fn reset_live(&mut self) {
        if self.run_active {
            self.drop_live_rows();
        } else {
            self.pending_reset = Some(false);
            self.bump();
        }
    }

    fn drop_live_rows(&mut self) {
        let keep = self.run_start.unwrap_or_else(|| {
            self.items
                .iter()
                .rposition(|i| matches!(i.kind, ItemKind::User { steered: false, .. }))
                .map_or(0, |p| p + 1)
        });
        self.items.truncate(keep.min(self.items.len()));
        self.run_start = Some(self.items.len());
        self.open = Open::None;
        self.rebuild_tool_index();
        self.bump();
    }

    fn rebuild_tool_index(&mut self) {
        self.tools = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, it)| match &it.kind {
                ItemKind::Tool(t) => Some((t.call_id.clone(), i)),
                _ => None,
            })
            .collect();
    }

    fn close_open(&mut self) {
        if let Open::Text(i) | Open::Thinking(i) = std::mem::replace(&mut self.open, Open::None)
            && let Some(
                ItemKind::Assistant { streaming, .. } | ItemKind::Thinking { streaming, .. },
            ) = self.items.get_mut(i).map(|it| &mut it.kind)
        {
            *streaming = false;
        }
    }

    fn end_run(&mut self) {
        self.close_open();
        if self.run_active {
            self.run_active = false;
            self.orphaned.extend(self.steering.run_ended());
        }
        // Nothing can answer a request once its run is over: fail closed.
        self.approvals.deny_all(DenyReason::ServerExpired);
        // Tools that never reported a result are not running anymore.
        for it in &mut self.items {
            if let ItemKind::Tool(t) = &mut it.kind
                && matches!(t.status, ToolStatus::Pending | ToolStatus::Running)
                && t.outcome.is_none()
            {
                t.status = ToolStatus::Failed;
            }
        }
    }

    /// Applies one chat event.
    pub fn apply(&mut self, ev: &ChatEvent) {
        if self.pending_reset.is_some()
            && !matches!(
                ev,
                ChatEvent::Session { .. } | ChatEvent::GoalStatus(_) | ChatEvent::Run { .. }
            )
        {
            self.pending_reset = None;
            self.drop_live_rows();
        }
        match ev {
            ChatEvent::Session {
                session_id,
                running,
            } => {
                if self.session_id.is_none() && !session_id.is_empty() {
                    self.session_id = Some(session_id.clone());
                }
                if *running {
                    self.run_active = true;
                } else if self.run_active {
                    self.end_run();
                }
            }
            ChatEvent::Run { run_seq, .. } => {
                if let Some(was_active) = self.pending_reset.take() {
                    if was_active || *run_seq <= self.run_seq {
                        self.drop_live_rows();
                    } else {
                        self.run_start = Some(self.items.len());
                        self.open = Open::None;
                    }
                }
                self.run_seq = *run_seq;
                self.seen_run_seq = self.seen_run_seq.max(*run_seq);
                self.run_active = true;
                if self.run_start.is_none() {
                    self.run_start = Some(self.items.len());
                }
            }
            ChatEvent::PermissionRequest(p) => {
                self.close_open();
                if self.approvals.request(&p.id).is_none() {
                    self.push(ItemKind::Approval {
                        request_id: p.id.clone(),
                    });
                }
                self.approvals
                    .register_permission(p.clone(), self.clock.now());
            }
            ChatEvent::QuestionRequest(q) => {
                self.close_open();
                if self.approvals.request(&q.id).is_none() {
                    self.push(ItemKind::Approval {
                        request_id: q.id.clone(),
                    });
                }
                self.approvals
                    .register_question(q.clone(), self.clock.now());
            }
            ChatEvent::ThinkingDelta(t) => self.delta(t, true),
            ChatEvent::ContentDelta(t) => self.delta(t, false),
            ChatEvent::ToolCall(c) | ChatEvent::ToolCallUpdate(c) => self.tool_call(c),
            ChatEvent::ToolResult(r) => self.tool_result(r),
            ChatEvent::PlanUpdate(p) => self.plan = p.clone(),
            ChatEvent::TodosUpdate(t) => self.todos = t.clone(),
            ChatEvent::TokenUsage(u) => self.usage.apply(u),
            ChatEvent::SystemMessage(m) => {
                self.close_open();
                let (kind, text) = if m.routing.is_some() {
                    (NoticeKind::Routing, m.text.trim().to_owned())
                } else {
                    (NoticeKind::System, m.text.trim().to_owned())
                };
                if !text.is_empty() {
                    self.push(ItemKind::Notice { kind, text });
                }
            }
            ChatEvent::SteeringQueued(_) => {} // the chip is already shown
            ChatEvent::SteeringInjected(n) => {
                self.close_open();
                let text = self
                    .steering
                    .injected(&n.message)
                    .unwrap_or_else(|| n.message.clone());
                self.push(ItemKind::User {
                    text,
                    steered: true,
                });
            }
            ChatEvent::ConclusionQueued(n) => self.notice(NoticeKind::ConclusionQueued, &n.message),
            ChatEvent::ConclusionInjected(n) => {
                self.notice(NoticeKind::ConclusionInjected, &n.message)
            }
            ChatEvent::Resurrected(n) => {
                self.run_active = true;
                self.notice(NoticeKind::Resurrected, &n.message);
            }
            ChatEvent::GoalStatus(g) => self.goal = g.clone(),
            ChatEvent::Error(msg) => {
                self.notice(NoticeKind::Error, msg);
                self.end_run();
            }
            ChatEvent::Done => self.end_run(),
            ChatEvent::Unknown { event, raw } => {
                let value =
                    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.clone()));
                self.push(ItemKind::Json {
                    label: event.clone(),
                    value,
                });
            }
        }
        self.bump();
    }

    fn notice(&mut self, kind: NoticeKind, text: &str) {
        self.close_open();
        self.push(ItemKind::Notice {
            kind,
            text: text.to_owned(),
        });
    }

    fn delta(&mut self, text: &str, thinking: bool) {
        match (self.open, thinking) {
            (Open::Text(i), false) | (Open::Thinking(i), true) => {
                if let Some(
                    ItemKind::Assistant { text: t, .. } | ItemKind::Thinking { text: t, .. },
                ) = self.items.get_mut(i).map(|it| &mut it.kind)
                {
                    t.push_str(text);
                }
            }
            _ => {
                self.close_open();
                let kind = if thinking {
                    ItemKind::Thinking {
                        text: text.to_owned(),
                        streaming: true,
                    }
                } else {
                    ItemKind::Assistant {
                        text: text.to_owned(),
                        streaming: true,
                    }
                };
                let i = self.push(kind);
                self.open = if thinking {
                    Open::Thinking(i)
                } else {
                    Open::Text(i)
                };
            }
        }
    }

    fn tool_call(&mut self, c: &ToolCallEvent) {
        self.close_open();
        let status = if c.status == "in_progress" {
            ToolStatus::Running
        } else {
            ToolStatus::Pending
        };
        let locations = c
            .locations
            .iter()
            .map(|l: &Location| l.path.clone())
            .collect();
        if let Some(&i) = self.tools.get(&c.id) {
            if let Some(ItemKind::Tool(t)) = self.items.get_mut(i).map(|it| &mut it.kind) {
                if t.outcome.is_none() {
                    t.status = status;
                }
                if !c.input.is_empty() {
                    t.input = c.input.clone();
                }
                if !c.title.is_empty() {
                    t.title = c.title.clone();
                }
                if !c.kind.is_empty() {
                    t.kind = c.kind.clone();
                }
            }
            return;
        }
        let i = self.push(ItemKind::Tool(ToolItem {
            call_id: c.id.clone(),
            name: c.name.clone(),
            kind: c.kind.clone(),
            title: c.title.clone(),
            status,
            input: c.input.clone(),
            locations,
            outcome: None,
            duration: None,
            started: Some(self.clock.now()),
        }));
        self.tools.insert(c.id.clone(), i);
    }

    fn tool_result(&mut self, r: &ToolResultEvent) {
        self.close_open();
        if !self.tools.contains_key(&r.tool_call_id) {
            let i = self.push(ItemKind::Tool(ToolItem {
                call_id: r.tool_call_id.clone(),
                name: r.name.clone(),
                kind: r.kind.clone(),
                title: r.title.clone(),
                status: ToolStatus::Running,
                input: String::new(),
                locations: Vec::new(),
                outcome: None,
                duration: None,
                started: None,
            }));
            self.tools.insert(r.tool_call_id.clone(), i);
        }
        let now = self.clock.now();
        let Some(&i) = self.tools.get(&r.tool_call_id) else {
            return;
        };
        if let Some(ItemKind::Tool(t)) = self.items.get_mut(i).map(|it| &mut it.kind) {
            t.status = if r.is_error || r.status == "failed" {
                ToolStatus::Failed
            } else {
                ToolStatus::Completed
            };
            if !r.title.is_empty() {
                t.title = r.title.clone();
            }
            if t.locations.is_empty() {
                t.locations = r.locations.iter().map(|l| l.path.clone()).collect();
            }
            t.duration = t.started.map(|s| now.saturating_sub(s));
            t.outcome = Some(ToolOutcome {
                content: r.content.clone(),
                is_error: r.is_error,
                diff: r.diff.clone(),
                terminal: r.terminal.clone(),
                images: r.images.clone().unwrap_or_default(),
                raw_output: r.raw_output.clone(),
            });
        }
    }

    // ------------------------------------------------------------ history (PDA-T-0065)

    /// Rebuilds the transcript from `GET /sessions/{id}`: tool results folded into their calls,
    /// unknown parts as JSON rows, images, finish reasons as notices, `[delegated-result]` blocks
    /// as conclusion rows. Live state (approvals, steering chips) is kept; run state follows the
    /// server (`is_running`, `run_seq`).
    pub fn load_history(&mut self, detail: &SessionDetail) {
        self.items.clear();
        self.tools.clear();
        self.open = Open::None;
        self.run_start = None;
        if self.session_id.is_none() && !detail.session.id.is_empty() {
            self.session_id = Some(detail.session.id.clone());
        }
        self.title = detail.session.title.clone();
        for m in fold_messages(&detail.messages) {
            for item in m.items {
                self.history_item(&m.role, item);
            }
        }
        self.run_active = detail.is_running;
        self.run_seq = detail.run_seq;
        self.seen_run_seq = seen_for(detail.is_running, detail.run_seq);
        self.usage.set_session_totals(&detail.session);
        self.rebuild_tool_index();
        self.bump();
    }

    fn history_item(&mut self, role: &str, item: FoldedItem) {
        match item {
            FoldedItem::Text(t) => {
                if t.contains("[delegated-result") {
                    let rows = parse_delegated_results(&t);
                    if !rows.is_empty() {
                        for r in rows {
                            self.push(ItemKind::Conclusion(Box::new(r)));
                        }
                        return;
                    }
                }
                if role == "user" {
                    self.push(ItemKind::User {
                        text: t,
                        steered: false,
                    });
                } else if role == "system" {
                    self.push(ItemKind::Notice {
                        kind: NoticeKind::System,
                        text: t,
                    });
                } else {
                    self.push(ItemKind::Assistant {
                        text: t,
                        streaming: false,
                    });
                }
            }
            FoldedItem::Thinking(t) => {
                self.push(ItemKind::Thinking {
                    text: t,
                    streaming: false,
                });
            }
            FoldedItem::Tool { call, result } => {
                let outcome = result.as_ref().map(|r| ToolOutcome {
                    content: r.content.clone(),
                    is_error: r.is_error,
                    raw_output: serde_json::from_str(&r.metadata).unwrap_or(Value::Null),
                    ..ToolOutcome::default()
                });
                let status = match &result {
                    Some(r) if r.is_error => ToolStatus::Failed,
                    Some(_) => ToolStatus::Completed,
                    None => ToolStatus::Failed,
                };
                let title = if call.name.is_empty() {
                    call.id.clone()
                } else {
                    call.name.clone()
                };
                self.push(ItemKind::Tool(ToolItem {
                    call_id: call.id,
                    name: call.name,
                    kind: String::new(),
                    title,
                    status,
                    input: call.input,
                    locations: Vec::new(),
                    outcome,
                    duration: None,
                    started: None,
                }));
            }
            FoldedItem::Image(url) => {
                self.push(ItemKind::Image { url });
            }
            FoldedItem::Binary { path, mime_type } => {
                self.push(ItemKind::Attachment { path, mime_type });
            }
            FoldedItem::Other(value) => {
                self.push(ItemKind::Json {
                    label: "part".to_owned(),
                    value,
                });
            }
            FoldedItem::Finish(reason) => {
                let kind = match reason.as_str() {
                    "canceled" | "cancelled" => Some(NoticeKind::Cancelled),
                    "max_tokens" => Some(NoticeKind::MaxTokens),
                    "permission_denied" => Some(NoticeKind::PermissionDenied),
                    "error" => Some(NoticeKind::Error),
                    _ => None, // end_turn, tool_use, unknown: nothing to say
                };
                if let Some(kind) = kind {
                    self.push(ItemKind::Notice { kind, text: reason });
                }
            }
        }
    }

    // ------------------------------------------------------------ snapshots

    /// A stable plain-text rendering of the whole state, for snapshot tests.
    pub fn render_debug(&self) -> String {
        let mut o = String::new();
        for it in &self.items {
            match &it.kind {
                ItemKind::User { text, steered } => {
                    o.push_str(&format!(
                        "user{}: {}\n",
                        if *steered { " (steered)" } else { "" },
                        text
                    ));
                }
                ItemKind::Assistant { text, streaming } => {
                    o.push_str(&format!(
                        "assistant{}: {}\n",
                        if *streaming { " ..." } else { "" },
                        short(text)
                    ));
                }
                ItemKind::Thinking { text, streaming } => {
                    o.push_str(&format!(
                        "thinking{}: {}\n",
                        if *streaming { " ..." } else { "" },
                        short(text)
                    ));
                }
                ItemKind::Tool(t) => {
                    o.push_str(&format!(
                        "tool {} [{:?}] kind={} input={}",
                        t.name,
                        t.status,
                        t.kind,
                        short(&t.input)
                    ));
                    if let Some(out) = &t.outcome {
                        o.push_str(&format!(
                            " -> {}{}",
                            if out.is_error { "ERROR " } else { "" },
                            short(&out.content)
                        ));
                    }
                    o.push('\n');
                }
                ItemKind::Approval { request_id } => {
                    let state = match self.approvals.resolution(request_id) {
                        None => "pending".to_owned(),
                        Some(r) => format!("{r:?}"),
                    };
                    o.push_str(&format!("approval {request_id}: {state}\n"));
                }
                ItemKind::Notice { kind, text } => {
                    o.push_str(&format!("notice {kind:?}: {}\n", short(text)))
                }
                ItemKind::Conclusion(c) => o.push_str(&format!(
                    "conclusion task={} status={} summary={}\n",
                    c.task,
                    c.status,
                    short(&c.summary)
                )),
                ItemKind::Image { url } => o.push_str(&format!("image {}\n", short(url))),
                ItemKind::Attachment { path, mime_type } => {
                    o.push_str(&format!("attachment {path} {mime_type}\n"))
                }
                ItemKind::Json { label, value } => {
                    o.push_str(&format!("json {label}: {}\n", short(&value.to_string())))
                }
            }
        }
        if !self.plan.is_empty() {
            o.push_str(&format!(
                "-- plan: {}\n",
                self.plan
                    .iter()
                    .map(|p| format!("{}({})", p.title, p.status))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.todos.is_empty() {
            o.push_str(&format!(
                "-- todos: {}\n",
                self.todos
                    .iter()
                    .map(|p| format!("{}({}/{})", p.content, p.status, p.priority))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if let Some(g) = self.goal_banner() {
            o.push_str(&format!(
                "-- goal: {} [{}] {}/{} cancel={}\n",
                g.objective, g.status, g.iteration, g.max_iterations, g.can_cancel
            ));
        }
        if let Some(u) = self.usage.current() {
            o.push_str(&format!(
                "-- usage: prompt={} completion={} cache_read={} cache_creation={} reasoning={} cost={:.6} window={} estimated={}\n",
                u.prompt_tokens, u.completion_tokens, u.cache_read_tokens, u.cache_creation_tokens, u.reasoning_tokens, u.cost, u.context_window, u.estimated
            ));
        }
        o.push_str(&format!(
            "-- running={} run_seq={} seen={}\n",
            self.run_active, self.run_seq, self.seen_run_seq
        ));
        o
    }
}

fn short(s: &str) -> String {
    let one: String = s.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    if one.chars().count() > 90 {
        let cut: String = one.chars().take(90).collect();
        format!("{cut}...")
    } else {
        one
    }
}
