//! `ChatPanel`: the native chat (sessions sidebar, transcript, composer, banners and dialogs).
//!
//! This file owns the state and the behaviour; the `render_*` modules only draw it. All work that
//! talks to Pando goes through [`ChatService`] and comes back as [`ChatMsg`]s applied by
//! [`ChatPanel::on_msg`] (one notification per burst, see [`EventPump`]).
//!
//! Safety (PDA-SP-0003): approvals are decided only by [`ChatPanel::decide_permission`] and
//! [`ChatPanel::send_answers`], which the dialog buttons call with [`Origin::Explicit`]. The
//! per-session auto-approve policy may call the ledger with [`Origin::Auto`], and the ledger
//! refuses that for every flagged request.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use pando_client::api::{CommandInfo, ModelList};
use pando_client::approvals::{Decision, Origin, RequestBody};
use pando_client::chat::{ChatEvent, Goal};
use pando_client::follow::FollowEvent;
use pando_client::model::ItemId;
use pando_client::steering::{SendPlan, SteerResult, StopPlan};
use pando_client::{ChatModel, PandoClient};
use rust_i18n::t;
use tokio::runtime::Handle;

use super::attention::{PendingTally, QuestionFlow, WaitingKind, auto_approvable, waiting_queue};
use super::composer::{self, Mention, PromptHistory};
use super::markdown::Block;
use super::service::{ChatMsg, ChatService, FollowHandle, PageMode, TaskGuard};
use super::sessions::SessionsState;
use crate::events::EventPump;
use crate::ui::input::{InputEvent, InputState, TextareaState};
use crate::ui::list::UniformListScrollHandle;
use crate::ui::{
    AppContext as _, Context, Entity, FocusHandle, ScrollHandle, Subscription, Task, Window,
};

/// A parsed markdown cache entry: `(text length it was parsed from, blocks)`.
pub(super) type MdEntry = (usize, Rc<Vec<Block>>);

/// State of an `@` mention popup.
#[derive(Debug, Clone)]
pub(super) struct MentionState {
    pub mention: Mention,
    pub results: Vec<String>,
    pub index: usize,
    pub requested: String,
}

/// An inline rename in the sidebar.
#[allow(missing_debug_implementations)]
pub(super) struct RenameState {
    pub id: String,
    pub old: String,
    pub input: Entity<InputState>,
    pub _sub: Subscription,
}

/// The chat view.
#[allow(missing_debug_implementations)]
pub struct ChatPanel {
    pub(super) handle: Handle,
    pub(super) tx: async_channel::Sender<ChatMsg>,
    pub(super) service: Option<ChatService>,
    pub(super) model: ChatModel,
    pub(super) epoch: u64,
    pub(super) loading: bool,
    pub(super) sessions: SessionsState,
    pub(super) follow: Option<FollowHandle>,
    pub(super) run: Option<TaskGuard>,
    pub(super) run_id: u64,
    pub(super) poller: Option<TaskGuard>,
    pub(super) inflight: HashSet<String>,
    pub(super) tally: PendingTally,
    // composer
    pub(super) composer: Entity<TextareaState>,
    pub(super) history: PromptHistory,
    pub(super) commands: Vec<CommandInfo>,
    pub(super) palette_ix: usize,
    pub(super) palette_dismissed: bool,
    pub(super) focus: FocusHandle,
    pub(super) dialog_up: bool,
    pub(super) mention: Option<MentionState>,
    pub(super) dictated: Option<Range<usize>>,
    pub(super) focus_composer: bool,
    // transcript
    pub(super) scroll: ScrollHandle,
    pub(super) tail: bool,
    pub(super) open: HashSet<ItemId>,
    pub(super) full: HashSet<ItemId>,
    pub(super) md: HashMap<ItemId, MdEntry>,
    pub(super) diff_file: Option<String>,
    pub(super) diff_split: bool,
    pub(super) banners_open: HashSet<&'static str>,
    // sidebar
    pub(super) search: Entity<InputState>,
    pub(super) rename: Option<RenameState>,
    pub(super) list_scroll: UniformListScrollHandle,
    pub(super) show_sidebar: bool,
    pub(super) sidebar_decided: bool,
    // models
    pub(super) models: Option<ModelList>,
    pub(super) session_model: Option<String>,
    pub(super) prev_model: Option<String>,
    pub(super) pending_model: Option<String>,
    pub(super) model_menu: bool,
    pub(super) model_filter: Entity<InputState>,
    // approvals
    pub(super) auto_approve: bool,
    pub(super) question_flow: Option<QuestionFlow>,
    pub(super) other_input: Entity<InputState>,
    // misc
    pub(super) usage_open: bool,
    pub(super) toast: Option<String>,
    pub(super) pending_goal: Option<Goal>,
    pub(super) now: i64,
    _pump: Task<()>,
    _ticker: Task<()>,
    _subs: Vec<Subscription>,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

impl ChatPanel {
    /// Builds the panel. Nothing talks to Pando until [`ChatPanel::set_client`].
    pub fn new(window: &mut Window, cx: &mut Context<Self>, handle: Handle) -> Self {
        let (tx, rx) = async_channel::unbounded();
        let pump = EventPump::attach(cx, rx, |this: &mut Self, msg, cx| this.on_msg(msg, cx));
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        });
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 8)
                .submit_on_enter(true)
                .placeholder(t!("chat.composer.placeholder").to_string())
        });
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("chat.sidebar.search").to_string())
        });
        let model_filter = cx
            .new(|cx| InputState::new(window, cx).placeholder(t!("chat.model.search").to_string()));
        let other_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("chat.questions.other_placeholder").to_string())
        });
        let mut subs = Vec::new();
        subs.push(cx.subscribe_in(
            &composer,
            window,
            |this: &mut Self, _, ev: &InputEvent, window, cx| {
                this.on_composer_event(ev, window, cx)
            },
        ));
        subs.push(cx.subscribe_in(
            &search,
            window,
            |this: &mut Self, state, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    let q = state.read(cx).value().to_string();
                    this.sessions.set_query(&q);
                    cx.notify();
                }
            },
        ));
        subs.push(cx.subscribe_in(
            &model_filter,
            window,
            |_: &mut Self, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    cx.notify();
                }
            },
        ));
        subs.push(cx.subscribe_in(
            &other_input,
            window,
            |this: &mut Self, state, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change)
                    && let Some(flow) = this.question_flow.as_mut()
                    && let Some(q) = flow.current()
                {
                    let text = state.read(cx).value().to_string();
                    flow.set_other_text(q, &text);
                    flow.set_other(q, !text.trim().is_empty() || flow.drafts()[q].other_on);
                    cx.notify();
                }
            },
        ));
        Self {
            handle,
            tx,
            service: None,
            model: ChatModel::default(),
            epoch: 0,
            loading: false,
            sessions: SessionsState::default(),
            follow: None,
            run: None,
            run_id: 0,
            poller: None,
            inflight: HashSet::new(),
            tally: PendingTally::default(),
            composer,
            history: PromptHistory::default(),
            commands: Vec::new(),
            palette_ix: 0,
            palette_dismissed: false,
            focus: cx.focus_handle(),
            dialog_up: false,
            mention: None,
            dictated: None,
            focus_composer: false,
            scroll: ScrollHandle::new(),
            tail: true,
            open: HashSet::new(),
            full: HashSet::new(),
            md: HashMap::new(),
            diff_file: None,
            diff_split: false,
            banners_open: HashSet::new(),
            search,
            rename: None,
            list_scroll: UniformListScrollHandle::new(),
            show_sidebar: true,
            sidebar_decided: false,
            models: None,
            session_model: None,
            prev_model: None,
            pending_model: None,
            model_menu: false,
            model_filter,
            auto_approve: false,
            question_flow: None,
            other_input,
            usage_open: false,
            toast: None,
            pending_goal: None,
            now: unix_now(),
            _pump: pump,
            _ticker: ticker,
            _subs: subs,
        }
    }

    // ------------------------------------------------------------------ connection

    /// Hands the panel a connected client (or none when Pando went away). A new client reloads
    /// the sidebar and the open session.
    pub fn set_client(&mut self, client: Option<PandoClient>, cx: &mut Context<Self>) {
        match client {
            Some(client) => {
                let same = self
                    .service
                    .as_ref()
                    .is_some_and(|s| s.client().base_url() == client.base_url());
                let service = ChatService::new(client, self.handle.clone(), self.tx.clone());
                self.service = Some(service.clone());
                if same && self.poller.is_some() {
                    // Token refresh or reconnect to the same server: keep the session as is.
                    return;
                }
                self.poller = Some(service.start_poller());
                service.list_commands();
                service.list_models();
                if let Some(id) = self.model.session_id().map(str::to_owned) {
                    self.reopen(&id);
                }
            }
            None => {
                self.service = None;
                self.poller = None;
                self.follow = None;
                self.run = None;
            }
        }
        cx.notify();
    }

    /// Whether the panel can talk to Pando.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.service.is_some()
    }

    fn reopen(&mut self, id: &str) {
        self.follow = None;
        self.run = None;
        self.reset_session_ui();
        self.epoch = self.model.begin_session(Some(id));
        self.loading = true;
        self.flush_outbox();
        if let Some(s) = &self.service {
            s.open_session(self.epoch, id);
        }
    }

    // ------------------------------------------------------------------ public API

    /// The decision layer dictated text: insert it at the cursor with a visible "dictado" mark.
    /// It never sends anything (PDA-T-0076).
    pub fn insert_dictation(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (current, cursor) = self.composer_text(cx);
        let d = composer::insert_dictation(&current, cursor, text);
        self.dictated = Some(d.mark.clone());
        self.set_composer_text(&d.text, window, cx);
        self.composer.update(cx, |s, cx| {
            s.set_cursor_position(composer::position(&d.text, d.cursor), window, cx);
        });
        self.focus_composer = true;
        cx.notify();
    }

    /// Pending items over all followed sessions (the global indicator, PDA-T-0083).
    #[must_use]
    pub fn pending_total(&self) -> usize {
        let own = waiting_queue(&self.model).len();
        let other: usize = self
            .tally
            .total()
            .saturating_sub(self.model.session_id().map_or(0, |id| self.tally.count(id)));
        own + other
    }

    /// Sessions with something waiting for the user.
    #[must_use]
    pub fn pending_sessions(&self) -> usize {
        let own = usize::from(!waiting_queue(&self.model).is_empty());
        let current = self.model.session_id().map(str::to_owned);
        let others = self.tally.sessions()
            - current
                .as_deref()
                .map_or(0, |id| usize::from(self.tally.count(id) > 0));
        own + others
    }

    /// Jumps to the first session waiting for the user (the indicator's click).
    pub fn open_first_pending(&mut self, cx: &mut Context<Self>) {
        if !waiting_queue(&self.model).is_empty() {
            return; // the dialog is already up
        }
        if let Some(id) = self.tally.first(self.model.session_id()).map(str::to_owned) {
            self.select_session(&id, cx);
        }
    }

    // ------------------------------------------------------------------ sessions

    /// Starts an empty conversation; the session is created by the first prompt.
    pub fn new_session(&mut self, cx: &mut Context<Self>) {
        self.follow = None;
        self.run = None;
        self.reset_session_ui();
        self.epoch = self.model.begin_session(None);
        self.loading = false;
        self.flush_outbox();
        self.focus_composer = true;
        cx.notify();
    }

    /// Opens a session.
    pub fn select_session(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.model.session_id() == Some(id) && !self.loading {
            return;
        }
        self.reopen(id);
        self.focus_composer = true;
        cx.notify();
    }

    fn reset_session_ui(&mut self) {
        self.open.clear();
        self.full.clear();
        self.md.clear();
        self.diff_file = None;
        self.tail = true;
        self.auto_approve = false;
        self.session_model = None;
        self.prev_model = None;
        self.question_flow = None;
        self.mention = None;
        self.usage_open = false;
        self.model_menu = false;
        self.toast = None;
        self.pending_goal = None;
    }

    /// Fetches the next page of sessions.
    pub fn load_more_sessions(&mut self, cx: &mut Context<Self>) {
        if self.sessions.is_loading() || !self.sessions.has_more() {
            return;
        }
        if let Some(s) = &self.service {
            self.sessions.set_loading(true);
            s.list_sessions(PageMode::Append, self.sessions.next_offset());
            cx.notify();
        }
    }

    /// Refreshes the first page of sessions.
    pub fn refresh_sessions(&mut self) {
        if let Some(s) = &self.service {
            s.list_sessions(PageMode::Head, 0);
        }
    }

    pub(super) fn delete_session(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(s) = &self.service {
            s.delete(id);
        }
        cx.notify();
    }

    pub(super) fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.rename.take() else { return };
        let title = r.input.read(cx).value().trim().to_owned();
        if !title.is_empty() && title != r.old {
            self.sessions.set_title(&r.id, &title);
            if self.model.session_id() == Some(r.id.as_str()) {
                self.toast = None;
            }
            if let Some(s) = &self.service {
                s.rename(&r.id, &title);
            }
        }
        cx.notify();
    }

    pub(super) fn start_rename(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let old = self
            .sessions
            .get(id)
            .and_then(super::sessions::display_title)
            .unwrap_or_default();
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(old.clone(), window, cx);
            state
        });
        let sub = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _, ev: &InputEvent, _, cx| match ev {
                InputEvent::PressEnter { .. } => this.commit_rename(cx),
                InputEvent::Blur => {
                    this.rename = None;
                    cx.notify();
                }
                _ => {}
            },
        );
        input.update(cx, |s, cx| s.focus(window, cx));
        self.rename = Some(RenameState {
            id: id.to_owned(),
            old,
            input,
            _sub: sub,
        });
        cx.notify();
    }

    // ------------------------------------------------------------------ composer

    pub(super) fn composer_text(&self, cx: &crate::ui::App) -> (String, usize) {
        let state = self.composer.read(cx);
        let text = state.value().to_string();
        let cursor = state.cursor().min(text.len());
        (text, cursor)
    }

    pub(super) fn set_composer_text(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = text.to_owned();
        self.composer
            .update(cx, |s, cx| s.set_value(text, window, cx));
    }

    fn on_composer_event(&mut self, ev: &InputEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            InputEvent::Change => {
                let (text, cursor) = self.composer_text(cx);
                self.history.reset();
                self.palette_ix = 0;
                self.palette_dismissed = false;
                // Typing over a dictated range ends the mark.
                if let Some(r) = &self.dictated
                    && (r.end > text.len() || text.get(r.clone()).is_none())
                {
                    self.dictated = None;
                }
                self.update_mention(&text, cursor);
                cx.notify();
            }
            InputEvent::PressEnter { shift, secondary } => {
                if *shift || *secondary {
                    return;
                }
                if self.accept_popup(window, cx) {
                    return;
                }
                self.send(window, cx);
            }
            _ => {}
        }
    }

    pub(super) fn update_mention(&mut self, text: &str, cursor: usize) {
        match composer::mention_at(text, cursor) {
            Some(m) => {
                let q = m.query.clone();
                let keep = self
                    .mention
                    .as_ref()
                    .map(|s| (s.requested.clone(), s.results.clone()));
                let (requested, results) = keep.unwrap_or_default();
                let mut state = MentionState {
                    mention: m,
                    results,
                    index: 0,
                    requested,
                };
                if !q.is_empty()
                    && state.requested != q
                    && let Some(s) = &self.service
                {
                    s.search_files(&q);
                    state.requested = q;
                }
                self.mention = Some(state);
            }
            None => self.mention = None,
        }
    }

    /// Enter with a popup open picks the highlighted entry instead of sending. Returns whether
    /// it did.
    fn accept_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(m) = &self.mention
            && let Some(path) = m.results.get(m.index).cloned()
        {
            self.pick_mention(&path, window, cx);
            return true;
        }
        let (text, _) = self.composer_text(cx);
        if !self.palette_dismissed
            && let Some(q) = composer::slash_query(&text)
        {
            let matches = composer::filter_commands(&self.commands, q);
            if let Some(c) = matches.get(self.palette_ix.min(matches.len().saturating_sub(1))) {
                let exact = c.name == q && !c.accepts_args;
                if !exact {
                    let text = composer::command_text(c);
                    self.set_composer_text(&text, window, cx);
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn pick_command(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = self.commands.iter().find(|c| c.name == name) {
            let text = composer::command_text(c);
            self.set_composer_text(&text, window, cx);
            self.focus_composer = true;
        }
    }

    pub(super) fn pick_mention(&mut self, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.mention.take() else {
            return;
        };
        let (text, cursor) = self.composer_text(cx);
        let (out, pos) = composer::apply_mention(&text, &state.mention, cursor, path);
        self.set_composer_text(&out, window, cx);
        self.composer.update(cx, |s, cx| {
            s.set_cursor_position(composer::position(&out, pos), window, cx);
        });
        self.focus_composer = true;
        cx.notify();
    }

    /// Sends the composer text: a prompt when idle, steering while a run is active.
    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (text, _) = self.composer_text(cx);
        if !composer::can_send(&text) || self.service.is_none() {
            return;
        }
        let text = text.trim().to_owned();
        self.history.push(&text);
        self.dictated = None;
        self.mention = None;
        self.toast = None;
        self.set_composer_text("", window, cx);
        self.submit_text(&text, cx);
    }

    pub(super) fn submit_text(&mut self, text: &str, cx: &mut Context<Self>) {
        match self.model.submit(text) {
            SendPlan::Prompt(t) => self.start_run(t),
            SendPlan::Steer { chip, text } => match (&self.service, self.model.session_id()) {
                (Some(s), Some(id)) => s.steer(self.epoch, id, chip, &text),
                (None, _) => {
                    let why = t!("chat.transcript.offline").to_string();
                    self.model.steering_mut().steer_failed(chip, &why);
                }
                (Some(_), None) => {
                    // No session to steer: never lose the text.
                    self.model.steering_mut().discard(chip);
                    self.start_run(text);
                }
            },
        }
        self.tail = true;
        cx.notify();
    }

    fn start_run(&mut self, prompt: String) {
        let Some(service) = self.service.clone() else {
            return;
        };
        let sid = self.model.session_id().map(str::to_owned);
        if let Some(f) = &self.follow {
            f.begin_local_run();
        }
        let model = if sid.is_none() {
            self.pending_model.clone()
        } else {
            None
        };
        self.run_id += 1;
        self.run = Some(service.send_prompt(self.epoch, self.run_id, sid, prompt, model));
    }

    /// Stop: cancels the run and the goal when there is one.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        let plan = self.model.stop_plan();
        self.stop_with(plan, cx);
    }

    pub(super) fn stop_with(&mut self, plan: StopPlan, cx: &mut Context<Self>) {
        if !plan.cancel_run && !plan.cancel_goal {
            return;
        }
        if let (Some(s), Some(id)) = (&self.service, self.model.session_id()) {
            s.stop(self.epoch, id, plan);
        }
        cx.notify();
    }

    // ------------------------------------------------------------------ approvals

    /// The user pressed a button on a permission dialog. Always [`Origin::Explicit`].
    pub fn decide_permission(&mut self, id: &str, decision: Decision, cx: &mut Context<Self>) {
        match self.model.decide(id, decision, Origin::Explicit) {
            Ok(_) => self.flush_outbox(),
            Err(why) => {
                tracing::warn!(?why, "decision refused");
                self.toast = Some(t!("chat.permission.refused").to_string());
            }
        }
        cx.notify();
    }

    /// Sends the answers of the questions stepper (only when complete).
    pub fn send_answers(&mut self, cx: &mut Context<Self>) {
        let Some(flow) = &self.question_flow else {
            return;
        };
        let (id, answers) = (flow.request_id().to_owned(), flow.answers());
        let Some(answers) = answers else { return };
        if self
            .model
            .decide(&id, Decision::Answer(answers), Origin::Explicit)
            .is_ok()
        {
            self.question_flow = None;
            self.flush_outbox();
        }
        cx.notify();
    }

    /// Cancels the questions (`cancelled: true`).
    pub fn cancel_questions(&mut self, cx: &mut Context<Self>) {
        let Some(flow) = self.question_flow.take() else {
            return;
        };
        let _ = self
            .model
            .decide(flow.request_id(), Decision::Deny, Origin::Explicit);
        // A question cannot be "denied" by the ledger's permission rules; cancel it explicitly.
        let still = self.model.approvals().is_pending(flow.request_id());
        if still {
            self.model
                .approvals_mut()
                .deny(flow.request_id(), pando_client::approvals::DenyReason::User);
        }
        self.flush_outbox();
        cx.notify();
    }

    pub(super) fn flush_outbox(&mut self) {
        let Some(service) = &self.service else { return };
        let out: Vec<_> = self
            .model
            .approvals()
            .outbox()
            .into_iter()
            .filter(|o| {
                let id = match o {
                    pando_client::approvals::Outgoing::Permission { id, .. }
                    | pando_client::approvals::Outgoing::Question { id, .. } => id,
                };
                self.inflight.insert(id.clone())
            })
            .collect();
        service.deliver(out);
    }

    /// Per-session auto-approve: approves the pending requests the policy allows, never the
    /// flagged ones (the ledger would refuse them anyway).
    fn apply_auto_approve(&mut self) {
        if !self.auto_approve {
            return;
        }
        let ids: Vec<String> = waiting_queue(&self.model)
            .into_iter()
            .filter(|w| w.kind == WaitingKind::Permission)
            .filter(|w| {
                matches!(
                    self.model.approvals().request(&w.id),
                    Some(RequestBody::Permission(p)) if auto_approvable(p)
                )
            })
            .map(|w| w.id)
            .collect();
        for id in ids {
            let _ = self.model.decide(&id, Decision::Approve, Origin::Auto);
        }
        self.flush_outbox();
    }

    /// Switches auto-approve for the open session (optimistic, rolled back on failure).
    pub fn toggle_auto_approve(&mut self, cx: &mut Context<Self>) {
        let want = !self.auto_approve;
        self.auto_approve = want;
        if want {
            self.apply_auto_approve();
        }
        if let (Some(s), Some(id)) = (&self.service, self.model.session_id()) {
            s.set_auto_approve(self.epoch, id, want);
        }
        cx.notify();
    }

    pub(super) fn sync_question_flow(&mut self) {
        let head = waiting_queue(&self.model).into_iter().next();
        match head {
            Some(w) if w.kind == WaitingKind::Question => {
                let current = self
                    .question_flow
                    .as_ref()
                    .map(|f| f.request_id().to_owned());
                if current.as_deref() != Some(w.id.as_str())
                    && let Some(RequestBody::Question(q)) = self.model.approvals().request(&w.id)
                {
                    self.question_flow = Some(QuestionFlow::new(q));
                }
            }
            _ => self.question_flow = None,
        }
    }

    // ------------------------------------------------------------------ models

    /// Chooses the model **of this session** (never the global one).
    pub fn select_model(&mut self, model: &str, cx: &mut Context<Self>) {
        self.model_menu = false;
        if let (Some(s), Some(id)) = (&self.service, self.model.session_id()) {
            self.prev_model = self.session_model.clone();
            self.session_model = Some(model.to_owned());
            s.set_session_model(self.epoch, id, model);
        } else {
            self.pending_model = Some(model.to_owned());
            self.session_model = Some(model.to_owned());
        }
        cx.notify();
    }

    // ------------------------------------------------------------------ timers

    fn tick(&mut self, cx: &mut Context<Self>) {
        self.now = unix_now();
        let expired = self.model.expire_approvals();
        if !expired.is_empty() {
            self.flush_outbox();
            cx.notify();
        } else if self.inflight.is_empty() && !self.model.approvals().outbox().is_empty() {
            self.flush_outbox();
        }
    }

    // ------------------------------------------------------------------ messages

    /// Applies one message of the service.
    pub fn on_msg(&mut self, msg: ChatMsg, cx: &mut Context<Self>) {
        // Text deltas are by far the most frequent message and cannot change approvals.
        let cheap = matches!(
            &msg,
            ChatMsg::Chat {
                ev: ChatEvent::ContentDelta(_) | ChatEvent::ThinkingDelta(_),
                ..
            } | ChatMsg::Follow {
                ev: FollowEvent::Chat(ChatEvent::ContentDelta(_) | ChatEvent::ThinkingDelta(_)),
                ..
            }
        );
        match msg {
            ChatMsg::Sessions { mode, result } => match result {
                Ok(page) => match mode {
                    PageMode::Replace => self.sessions.replace(page),
                    PageMode::Head => self.sessions.refresh_head(page),
                    PageMode::Append => self.sessions.append(page),
                },
                Err(e) => self.sessions.set_error(Some(e)),
            },
            ChatMsg::Opened { epoch, result } if epoch == self.epoch => {
                self.loading = false;
                match result {
                    Ok(detail) => {
                        self.model.load_history(&detail);
                        if let Some(g) = self.pending_goal.take() {
                            self.model.apply(&ChatEvent::GoalStatus(Some(g)));
                        }
                        if let (Some(s), Some(id)) = (&self.service, self.model.session_id()) {
                            self.follow = Some(s.follow(self.epoch, id, self.model.seen_run_seq()));
                        }
                        self.tail = true;
                    }
                    Err(e) => {
                        self.toast = Some(t!("chat.history.failed", error = e).to_string());
                    }
                }
            }
            ChatMsg::OpenedGoal {
                epoch,
                goal: Some(g),
            } if epoch == self.epoch => {
                if self.loading {
                    self.pending_goal = Some(g);
                } else {
                    self.model.apply(&ChatEvent::GoalStatus(Some(g)));
                }
            }
            ChatMsg::OpenedPending { epoch, events } if epoch == self.epoch => {
                for ev in &events {
                    self.model.apply(ev);
                }
                self.after_events();
            }
            ChatMsg::Chat { epoch, run, ev } if epoch == self.epoch && run == self.run_id => {
                self.on_chat_event(&ev, cx);
            }
            ChatMsg::ChatEnded { epoch, run, error }
                if epoch == self.epoch && run == self.run_id =>
            {
                self.run = None;
                if let Some(e) = error {
                    self.model.apply(&ChatEvent::Error(e));
                    if let Some(f) = &self.follow {
                        f.observe(&ChatEvent::Error(String::new()));
                    }
                    self.after_run_end(cx);
                } else if self.model.is_running()
                    && let (Some(s), Some(id)) = (&self.service, self.model.session_id())
                {
                    // The stream closed without `done`: let the follower replay the run.
                    self.follow = Some(s.follow(self.epoch, id, -1));
                }
            }
            ChatMsg::Follow { epoch, ev } if epoch == self.epoch => {
                let finished = matches!(ev, FollowEvent::RunFinished);
                // A reload of the history would wipe what this very app is streaming.
                let stale_reload =
                    matches!(ev, FollowEvent::HistoryReload(_)) && self.run.is_some();
                if !stale_reload {
                    self.model.apply_follow(&ev);
                }
                self.after_events();
                if finished {
                    self.after_run_end(cx);
                }
            }
            ChatMsg::Steer {
                epoch,
                chip,
                result,
            } if epoch == self.epoch => match result {
                Ok(SteerResult::Accepted) => self.model.steering_mut().steer_accepted(chip),
                Ok(SteerResult::NotBusy) => {
                    // The run ended before the steer arrived: it becomes a normal prompt.
                    if let Some(text) = self.model.steering_mut().steer_not_busy(chip) {
                        self.model.start_local_run(&text);
                        self.start_run(text);
                    }
                }
                Err(e) => self.model.steering_mut().steer_failed(chip, &e),
            },
            ChatMsg::Stopped {
                epoch,
                error: Some(e),
            } if epoch == self.epoch => {
                self.toast = Some(t!("chat.stop.failed", error = e).to_string());
            }
            ChatMsg::Delivered { sent, ok } => {
                for id in &sent {
                    self.inflight.remove(id);
                }
                for id in &ok {
                    self.model.approvals_mut().mark_delivered(id);
                }
            }
            ChatMsg::Models(Ok(list)) => self.models = Some(list),
            ChatMsg::Commands(Ok(c)) => self.commands = c,
            ChatMsg::SessionModel {
                epoch,
                result: Ok(m),
            } if epoch == self.epoch => self.session_model = Some(m.model),
            ChatMsg::ModelSet {
                epoch,
                model,
                result,
            } if epoch == self.epoch => match result {
                Ok(_) => {
                    self.session_model = Some(model);
                    self.prev_model = None;
                }
                Err(e) => {
                    self.session_model = self.prev_model.take();
                    self.toast = Some(t!("chat.model.failed", error = e).to_string());
                }
            },
            ChatMsg::AutoApprove {
                epoch,
                requested,
                result,
            } if epoch == self.epoch => match result {
                Ok(on) => {
                    self.auto_approve = on;
                    if on {
                        self.apply_auto_approve();
                    }
                }
                Err(e) => {
                    if let Some(want) = requested {
                        self.auto_approve = !want;
                        self.toast = Some(t!("chat.auto.failed", error = e).to_string());
                    }
                }
            },
            ChatMsg::Files { query, result } => {
                if let (Some(m), Ok(paths)) = (self.mention.as_mut(), result)
                    && m.requested == query
                {
                    m.results = paths;
                    m.index = 0;
                }
            }
            ChatMsg::Renamed { error: Some(e), .. } => {
                self.toast = Some(t!("chat.sidebar.rename_failed", error = e).to_string());
                self.refresh_sessions();
            }
            ChatMsg::Deleted { id, error } => match error {
                None => {
                    self.sessions.remove(&id);
                    self.tally.set(&id, 0);
                    if self.model.session_id() == Some(id.as_str()) {
                        self.new_session(cx);
                    }
                }
                Some(e) => {
                    self.toast = Some(t!("chat.sidebar.delete_failed", error = e).to_string())
                }
            },
            ChatMsg::PendingPoll(counts) => {
                let current = self.model.session_id().map(str::to_owned);
                let mut keep = std::collections::BTreeSet::new();
                for (id, n) in counts {
                    if current.as_deref() != Some(id.as_str()) {
                        self.tally.set(&id, n);
                    }
                    keep.insert(id);
                }
                self.tally.retain(&keep);
            }
            // Messages of a session that is no longer open.
            _ => {}
        }
        if !cheap {
            self.sync_question_flow();
        }
        cx.notify();
    }

    fn on_chat_event(&mut self, ev: &ChatEvent, cx: &mut Context<Self>) {
        let first_session =
            matches!(ev, ChatEvent::Session { .. }) && self.model.session_id().is_none();
        if let Some(f) = &self.follow {
            f.observe(ev);
        }
        self.model.apply(ev);
        if first_session && let Some(id) = self.model.session_id().map(str::to_owned) {
            self.sessions.insert_new(&id, "", true);
            if let Some(s) = &self.service {
                let handle = s.follow(self.epoch, &id, 1);
                handle.begin_local_run();
                self.follow = Some(handle);
                if self.auto_approve {
                    s.set_auto_approve(self.epoch, &id, true);
                }
                if let Some(m) = self.pending_model.take() {
                    self.session_model = Some(m);
                }
            }
        }
        if !matches!(ev, ChatEvent::ContentDelta(_) | ChatEvent::ThinkingDelta(_)) {
            self.after_events();
        }
        if matches!(ev, ChatEvent::Done | ChatEvent::Error(_)) {
            self.after_run_end(cx);
        }
    }

    fn after_events(&mut self) {
        self.apply_auto_approve();
        if let Some(id) = self.model.session_id().map(str::to_owned) {
            self.sessions.set_running(&id, self.model.is_running());
        }
    }

    fn after_run_end(&mut self, cx: &mut Context<Self>) {
        self.run = None;
        self.flush_outbox();
        self.refresh_sessions();
        // Steering text whose run ended before it was injected is sent as a normal prompt.
        for text in self.model.take_orphaned_steering() {
            self.submit_text(&text, cx);
        }
    }
}
