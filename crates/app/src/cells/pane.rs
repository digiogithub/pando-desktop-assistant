//! The organism pane of the main window (PDA-T-0055..0062): the organism canvas fed by the
//! cell source, the side card of the selected cell with its actions, the create-agent form and
//! the voice target.

use std::collections::{HashMap, HashSet};
use std::future::Future;

use organism::CellId;
use pando_client::approvals::{Approvals, Clock as _, Decision, DenyReason, Origin, SystemClock};
use pando_client::connection::Snapshot;
use pando_client::{PandoClient, Result as ClientResult};
use rust_i18n::t;
use tokio::runtime::Handle;
use tokio::sync::watch;

use super::actions::{self, ActionKind, Failure};
use super::card::{Card, CellAction, CellKind, kind_of};
use super::mapping::CustomEngines;
use super::new_agent::{DEFAULT_ENGINES, FormError, NewAgentForm};
use super::poller::{PollMsg, Poller};
use super::source::{CellSource, Poll};
use super::target::{TalkRefused, Target, TargetModel};
use crate::events::EventPump;
use crate::organism_view::{OrganismEvent, OrganismView, tr};
use crate::tokio_bridge;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::{Input, InputState};
use crate::ui::theme::palette;
use crate::ui::{
    AppContext as _, Confirmation, Context, Disableable as _, Entity, Hsla,
    InteractiveElement as _, IntoElement, Level, ParentElement as _, Render, Selectable as _,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div, h_flex, px, rgb,
    rgba, v_flex,
};

/// Weak handle to the live pane, so the shell can select a cell (OS notification click).
#[allow(missing_debug_implementations)]
struct PaneHandle(crate::ui::WeakEntity<OrganismPane>);

impl crate::ui::Global for PaneHandle {}

/// Selects `id` in the live pane (no-op when there is none).
pub fn select_cell(cx: &mut crate::ui::App, id: &str) {
    let Some(h) = cx.try_global::<PaneHandle>().map(|h| h.0.clone()) else {
        return;
    };
    h.update(cx, |pane, cx| pane.select_cell(id, cx)).ok();
}

/// Key context of the pane.
pub const KEY_CONTEXT: &str = "OrganismPane";

crate::ui::actions!(organism_pane, [BackToPrimary]);

/// Registers the pane's key bindings (call once at startup, next to
/// `organism_view::bind_keys`).
pub fn bind_keys(cx: &mut crate::ui::App) {
    if let Ok(b) = crate::ui::key_binding("escape", Box::new(BackToPrimary), Some(KEY_CONTEXT)) {
        cx.bind_keys([b]);
    }
}

/// The "new agent" form widgets.
struct FormState {
    prompt: Entity<InputState>,
    work_dir: Entity<InputState>,
    model: Entity<InputState>,
    tags: Entity<InputState>,
    engine: String,
}

/// The pane.
#[allow(missing_debug_implementations)]
pub struct OrganismPane {
    organism: Entity<OrganismView>,
    source: CellSource,
    poller: Poller,
    snapshot: watch::Receiver<Snapshot>,
    target: TargetModel,
    selected: Option<CellId>,
    approvals: HashMap<String, Approvals>,
    delivering: bool,
    primary_session: Option<String>,
    form: Option<FormState>,
    full_output: bool,
    /// Compact (split layout): no info panel, the canvas keeps the room.
    compact: bool,
    notice: Option<String>,
    custom_engines: Vec<String>,
    clock: SystemClock,
    _pump: Task<()>,
    _sub: Subscription,
    _activation: Subscription,
    _toasts: crate::shell::notify::ToastService,
    _toast_pump: Task<()>,
}

fn marfil(c: palette::Rgba) -> Hsla {
    rgba(c.as_u32()).into()
}

impl OrganismPane {
    /// Builds the pane and starts polling while `snapshot` has a client.
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        handle: &Handle,
        snapshot: watch::Receiver<Snapshot>,
        custom: CustomEngines,
        reduced_motion: bool,
    ) -> Self {
        let organism = cx.new(|cx| OrganismView::new(window, cx));
        organism.update(cx, |o, cx| o.set_reduced_motion(reduced_motion, cx));
        let sub = cx.subscribe(&organism, |this, _, ev: &OrganismEvent, cx| {
            match ev {
                OrganismEvent::CellSelected(id) => this.selected = Some(id.clone()),
                OrganismEvent::Deselected => {
                    this.selected = None;
                    this.target.back();
                }
            }
            cx.notify();
        });
        let weak = cx.weak_entity();
        cx.set_global(PaneHandle(weak));
        let activation = cx.observe_window_activation(window, |_, window, cx| {
            crate::shell::runtime::on_main_activation(cx, window.is_window_active());
        });
        // Toasts from /notifications/stream.
        let (toast_tx, toast_rx) = async_channel::unbounded::<crate::shell::notify::ToastSpec>();
        let toasts = crate::shell::notify::ToastService::start(handle, snapshot.clone(), toast_tx);
        let toast_pump = cx.spawn_in(window, async move |this, cx| {
            while let Ok(t) = toast_rx.recv().await {
                let alive = this.update_in(cx, |_, window, cx| {
                    use crate::shell::notify::ToastLevel;
                    let level = match t.level {
                        ToastLevel::Info => Level::Info,
                        ToastLevel::Warning => Level::Warning,
                        ToastLevel::Error => Level::Error,
                    };
                    crate::ui::notify(window, cx, level, t.message.clone());
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        let (tx, rx) = async_channel::unbounded();
        let poller = Poller::start(handle, snapshot.clone(), tx);
        let pump = EventPump::attach(cx, rx, |this: &mut Self, msg, cx| match msg {
            PollMsg::Poll(poll) => this.apply_poll(&poll, cx),
            PollMsg::Disconnected => this.disconnected(cx),
        });
        Self {
            organism,
            source: CellSource::new(custom, &t!("cells.primary")),
            poller,
            snapshot,
            target: TargetModel::new(),
            selected: None,
            approvals: HashMap::new(),
            delivering: false,
            primary_session: None,
            form: None,
            full_output: false,
            compact: false,
            notice: None,
            custom_engines: Vec::new(),
            clock: SystemClock::default(),
            _pump: pump,
            _sub: sub,
            _activation: activation,
            _toasts: toasts,
            _toast_pump: toast_pump,
        }
    }

    /// The chat session of the primary (mesnada) agent, whose pending permissions make the
    /// primary cell wait. The chat view calls this when the active session changes.
    pub fn set_primary_session(&mut self, id: Option<String>) {
        self.poller.set_primary_session(id.clone());
        self.primary_session = id;
    }

    /// Compact mode (split layout): hides the info panel.
    pub fn set_compact(&mut self, compact: bool, cx: &mut Context<Self>) {
        if self.compact != compact {
            self.compact = compact;
            cx.notify();
        }
    }

    /// Window visibility: slows polling down while hidden.
    pub fn set_visible(&self, visible: bool) {
        self.poller.set_visible(visible);
    }

    /// Who the next message goes to.
    #[must_use]
    pub fn target(&self) -> &TargetModel {
        &self.target
    }

    /// The selected cell, if any.
    #[must_use]
    pub fn selected(&self) -> Option<&CellId> {
        self.selected.as_ref()
    }

    /// Whether any permission waits for an answer.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.approvals.values().any(|a| !a.pending_ids().is_empty())
    }

    /// The organism entity (the app wires levels / body state through it).
    #[must_use]
    pub fn organism(&self) -> &Entity<OrganismView> {
        &self.organism
    }

    /// Text of the header's target pill.
    #[must_use]
    pub fn target_label(&self) -> String {
        match self.target.target() {
            Target::Primary => t!("cells.target.primary").to_string(),
            Target::Cell { label, .. } => t!("cells.target.cell", label = label).to_string(),
        }
    }

    /// Selects a cell programmatically (notification click, voice).
    pub fn select_cell(&mut self, id: &str, cx: &mut Context<Self>) {
        let id = CellId::new(id);
        if self.source.cells().iter().any(|c| c.id == id) {
            self.organism.update(cx, |o, cx| o.select(id, cx));
        }
    }

    // ---------------------------------------------------------------- polling

    fn disconnected(&mut self, cx: &mut Context<Self>) {
        self.source.reset();
        self.approvals.clear();
        self.selected = None;
        self.target.back();
        self.organism
            .update(cx, |o, cx| o.set_cells(Vec::new(), cx));
        cx.notify();
    }

    /// Applies one poll: ledger, cells, Delegating, selection and target.
    pub fn apply_poll(&mut self, poll: &Poll, cx: &mut Context<Self>) {
        self.sync_approvals(poll);
        let update = self.source.update(poll);
        if update.changed || !update.born.is_empty() {
            let cells = update.cells.clone();
            let delegate = update.delegate;
            self.organism.update(cx, |o, cx| {
                o.set_cells(cells, cx);
                if delegate {
                    o.delegate(cx);
                }
            });
        }
        self.target.on_cells(&update.cells);
        if let Some(sel) = &self.selected
            && !update.cells.iter().any(|c| &c.id == sel)
        {
            self.selected = None;
            self.organism.update(cx, |o, cx| o.set_selected(None, cx));
        }
        self.deliver(cx);
        let pending = self
            .approvals
            .values()
            .map(|a| a.pending_ids().len() as u32)
            .sum();
        let state = self.organism.read(cx).core().borrow().state;
        crate::shell::runtime::publish(cx, &update.cells, pending, state);
        cx.notify();
    }

    /// Registers new permissions; anything the server no longer lists is resolved as denied
    /// (fail-closed) and never sent back, and overdue ones time out.
    fn sync_approvals(&mut self, poll: &Poll) {
        let now = self.clock.now();
        let mut listed: HashMap<&str, HashSet<&str>> = HashMap::new();
        for req in &poll.permissions {
            listed
                .entry(req.session_id.as_str())
                .or_default()
                .insert(req.id.as_str());
            self.approvals
                .entry(req.session_id.clone())
                .or_default()
                .register_permission(req.clone(), now);
        }
        for (session, ledger) in &mut self.approvals {
            let still = listed.get(session.as_str());
            for id in ledger.pending_ids() {
                if !still.is_some_and(|s| s.contains(id.as_str()))
                    && ledger.deny(&id, DenyReason::ServerExpired)
                {
                    ledger.mark_delivered(&id);
                }
            }
            ledger.expire(now);
        }
    }

    // ---------------------------------------------------------------- calls to Pando

    fn client(&self) -> Option<PandoClient> {
        self.snapshot.borrow().client.clone()
    }

    /// Runs a call on the tokio runtime; failures become toasts (a 409 on cancel is only
    /// informational: the task had already finished).
    fn run<F, Fut>(
        &mut self,
        kind: ActionKind,
        window: &mut Window,
        cx: &mut Context<Self>,
        call: F,
    ) where
        F: FnOnce(PandoClient) -> Fut,
        Fut: Future<Output = ClientResult<()>> + Send + 'static,
    {
        let Some(client) = self.client() else {
            crate::ui::notify(
                window,
                cx,
                Level::Warning,
                t!("cells.new.not_connected").to_string(),
            );
            return;
        };
        let task = tokio_bridge::spawn(cx, call(client));
        cx.spawn_in(window, async move |this, cx| {
            let outcome = task.await;
            this.update_in(cx, |this, window, cx| {
                let failure = match outcome {
                    Ok(Ok(())) => None,
                    Ok(Err(e)) => Some(actions::classify(kind, &e)),
                    Err(e) => Some(Failure::Failed(e.to_string())),
                };
                match failure {
                    None => this.notice = None,
                    Some(Failure::AlreadyFinished) => {
                        crate::ui::notify(
                            window,
                            cx,
                            Level::Info,
                            t!("cells.error.already_finished").to_string(),
                        );
                    }
                    Some(Failure::Gone) => {
                        crate::ui::notify(
                            window,
                            cx,
                            Level::Info,
                            t!("cells.error.gone").to_string(),
                        );
                    }
                    Some(Failure::Failed(msg)) => {
                        crate::ui::notify(
                            window,
                            cx,
                            Level::Error,
                            t!("cells.error.action_failed", error = msg).to_string(),
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Sends the ledger's undelivered answers to Pando.
    fn deliver(&mut self, cx: &mut Context<Self>) {
        if self.delivering {
            return;
        }
        let outbox: Vec<_> = self
            .approvals
            .values()
            .flat_map(Approvals::outbox)
            .collect();
        if outbox.is_empty() {
            return;
        }
        let Some(client) = self.client() else { return };
        self.delivering = true;
        let task = tokio_bridge::spawn(cx, async move { actions::deliver(&client, outbox).await });
        cx.spawn(async move |this, cx| {
            let delivered = task.await.unwrap_or_default();
            this.update(cx, |this, cx| {
                for ledger in this.approvals.values_mut() {
                    for id in &delivered {
                        ledger.mark_delivered(id);
                    }
                }
                this.delivering = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The user pressed Approve / Deny on a pending request: an explicit decision in the
    /// fail-closed ledger, then delivery.
    pub fn decide(&mut self, session: &str, id: &str, approve: bool, cx: &mut Context<Self>) {
        let Some(ledger) = self.approvals.get_mut(session) else {
            return;
        };
        let decision = if approve {
            Decision::Approve
        } else {
            Decision::Deny
        };
        match ledger.decide(id, decision, Origin::Explicit) {
            Ok(_) => self.deliver(cx),
            Err(refused) => {
                self.notice = Some(
                    t!("cells.error.action_failed", error = format!("{refused:?}")).to_string(),
                );
            }
        }
        cx.notify();
    }

    // ---------------------------------------------------------------- actions

    fn session_of(&self, id: &CellId) -> Option<String> {
        match kind_of(id) {
            CellKind::Primary => self.primary_session.clone(),
            CellKind::Task => self
                .source
                .task(id.as_str())
                .and_then(|t| t.acp_session_id.clone())
                .filter(|s| !s.is_empty()),
            CellKind::Instance => None,
        }
    }

    fn selected_card(&self) -> Option<(Card, Option<String>)> {
        let id = self.selected.as_ref()?;
        let cell = self.source.cells().iter().find(|c| &c.id == id)?;
        let session = self.session_of(id);
        let pending: Vec<_> = session
            .as_deref()
            .and_then(|s| self.approvals.get(s))
            .map(|l| {
                l.pending_ids()
                    .into_iter()
                    .filter_map(|pid| match l.request(&pid) {
                        Some(pando_client::approvals::RequestBody::Permission(p)) => {
                            Some(p.clone())
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some((
            Card::build(cell, self.source.task(id.as_str()), &pending),
            session,
        ))
    }

    /// Runs a card action (Cancel / Delete ask first; Approve / Deny are per request).
    pub fn perform(&mut self, action: CellAction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let label = self
            .source
            .cells()
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.label.to_string())
            .unwrap_or_default();
        let weak = cx.weak_entity();
        match action {
            CellAction::Cancel | CellAction::Delete => {
                let cancel = action == CellAction::Cancel;
                let key = if cancel { "cancel" } else { "delete" };
                let question = Confirmation {
                    title: tr(&format!("cells.confirm.{key}_title")),
                    description: t!(&format!("cells.confirm.{key}_body"), label = label)
                        .to_string(),
                    ok_text: tr(&format!("cells.confirm.{key}_ok")),
                    cancel_text: t!("cells.confirm.keep").to_string(),
                };
                let task_id = id.as_str().to_owned();
                crate::ui::confirm(window, cx, question, move |window, cx| {
                    let task_id = task_id.clone();
                    weak.update(cx, |this, cx| {
                        if cancel {
                            this.run(ActionKind::Cancel, window, cx, move |c| async move {
                                actions::cancel(&c, &task_id).await
                            });
                        } else {
                            this.run(ActionKind::Delete, window, cx, move |c| async move {
                                actions::delete(&c, &task_id).await
                            });
                        }
                    })
                    .ok();
                });
            }
            CellAction::Relaunch => {
                if let Some(t) = self.source.task(id.as_str()).cloned() {
                    self.run(ActionKind::Create, window, cx, move |c| async move {
                        actions::relaunch(&c, &t).await.map(|_| ())
                    });
                }
            }
            CellAction::Talk => self.talk(&id, cx),
            // Approve / Deny act on a specific request: see `decide`.
            CellAction::Approve | CellAction::Deny => {}
        }
    }

    fn talk(&mut self, id: &CellId, cx: &mut Context<Self>) {
        let Some(cell) = self.source.cells().iter().find(|c| &c.id == id).cloned() else {
            return;
        };
        let session = self.session_of(id);
        self.notice = match self.target.talk_to(&cell, session) {
            Ok(()) => None,
            Err(TalkRefused::CannotTalk) => Some(t!("cells.error.talk_cli").to_string()),
            Err(TalkRefused::Finished) => Some(t!("cells.error.talk_finished").to_string()),
        };
        cx.notify();
    }

    /// Esc: the next message goes to the primary session again.
    pub fn back_to_primary(&mut self, cx: &mut Context<Self>) {
        if self.target.back() {
            cx.notify();
        }
    }

    // ---------------------------------------------------------------- create form

    fn open_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mk = |placeholder: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        self.form = Some(FormState {
            prompt: mk(t!("cells.new.prompt").to_string(), window, cx),
            work_dir: mk(t!("cells.new.work_dir").to_string(), window, cx),
            model: mk(t!("cells.new.model").to_string(), window, cx),
            tags: mk(t!("cells.new.tags").to_string(), window, cx),
            engine: DEFAULT_ENGINES[0].to_owned(),
        });
        self.selected = None;
        cx.notify();
    }

    fn submit_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = &self.form else { return };
        let form = NewAgentForm {
            prompt: f.prompt.read(cx).value().to_string(),
            engine: f.engine.clone(),
            work_dir: f.work_dir.read(cx).value().to_string(),
            model: f.model.read(cx).value().to_string(),
            tags: f.tags.read(cx).value().to_string(),
        };
        match form.to_request() {
            Err(FormError::EmptyPrompt) => {
                self.notice = Some(t!("cells.new.empty_prompt").to_string());
            }
            Ok(body) => {
                self.form = None;
                self.run(ActionKind::Create, window, cx, move |c| async move {
                    actions::create(&c, &body).await.map(|_| ())
                });
            }
        }
        cx.notify();
    }

    /// Engines the form offers: the built-ins plus the custom ones.
    fn engines(&self) -> Vec<String> {
        DEFAULT_ENGINES
            .iter()
            .map(|s| (*s).to_owned())
            .chain(self.custom_engines.iter().cloned())
            .collect()
    }

    /// Names of the custom engines to offer in the form.
    pub fn set_custom_engine_names(&mut self, names: Vec<String>) {
        self.custom_engines = names;
    }
}

// -------------------------------------------------------------------- rendering

fn heading(text: String) -> impl IntoElement {
    div()
        .mt(px(12.0))
        .text_size(px(11.0))
        .font_family(crate::fonts::FONT_MONO)
        .text_color(marfil(palette::MARFIL_ALPHA_62))
        .child(text)
}

impl OrganismPane {
    fn card_view(
        &self,
        card: Card,
        session: Option<String>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let weak = cx.weak_entity();
        let engine = tr(&format!("organism.engine.{}", card.engine_id));
        let status = tr(&format!("organism.status.{}", card.status_id));
        let mut col = v_flex()
            .id("cell-card")
            .gap(px(4.0))
            .p(px(16.0))
            .w(px(320.0))
            .h_full()
            .flex_shrink_0()
            .overflow_y_scroll()
            .border_l_1()
            .border_color(marfil(palette::MARFIL_ALPHA_12))
            .child(div().text_size(px(18.0)).child(card.label.clone()))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(marfil(palette::MARFIL_ALPHA_62))
                    .child(format!("{engine} · {status}")),
            );
        if let Some(p) = &card.project {
            col = col
                .child(heading(t!("cells.card.project").to_string()))
                .child(p.clone());
        }
        if let Some(pct) = card.progress {
            col = col
                .child(heading(t!("cells.card.progress").to_string()))
                .child(format!("{pct} %"));
        }
        if let Some(tool) = &card.current_tool {
            col = col
                .child(heading(t!("cells.card.tool").to_string()))
                .child(tool.clone());
        }
        if let Some(c) = &card.conclusion {
            col = col
                .child(heading(t!("cells.card.conclusion").to_string()))
                .child(format!("{} · {} %", c.status, c.confidence))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(marfil(palette::MARFIL_ALPHA_62))
                        .child(c.summary.clone()),
                );
        }
        col = col.child(heading(t!("cells.card.output").to_string()));
        if self.full_output {
            col = col.child(
                div()
                    .id("full-output")
                    .max_h(px(260.0))
                    .overflow_y_scroll()
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .bg(rgb(palette::BOSQUE_DEEP))
                    .font_family(crate::fonts::FONT_MONO)
                    .text_size(px(11.0))
                    .child(card.full_output.clone()),
            );
        } else if card.tail.is_empty() {
            col = col.child(
                div()
                    .text_color(marfil(palette::MARFIL_ALPHA_45))
                    .child(t!("cells.card.no_output").to_string()),
            );
        } else {
            col = col.child(
                v_flex()
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .bg(rgb(palette::BOSQUE_DEEP))
                    .font_family(crate::fonts::FONT_MONO)
                    .text_size(px(11.0))
                    .children(card.tail.iter().cloned().map(|l| div().child(l))),
            );
        }
        if !card.full_output.trim().is_empty() {
            let weak = weak.clone();
            let label = if self.full_output {
                t!("cells.card.hide_output")
            } else {
                t!("cells.card.full_output")
            };
            col = col.child(
                Button::new("full-output-toggle")
                    .ghost()
                    .label(label.to_string())
                    .on_click(move |_, _, cx| {
                        weak.update(cx, |t, cx| {
                            t.full_output = !t.full_output;
                            cx.notify();
                        })
                        .ok();
                    }),
            );
        }
        if !card.pending.is_empty() {
            col = col.child(heading(t!("cells.card.pending").to_string()));
            for (i, p) in card.pending.iter().enumerate() {
                let (w1, w2) = (weak.clone(), weak.clone());
                let (s1, s2) = (
                    session.clone().unwrap_or_default(),
                    session.clone().unwrap_or_default(),
                );
                let (id1, id2) = (p.id.clone(), p.id.clone());
                col = col.child(
                    v_flex()
                        .gap(px(4.0))
                        .p(px(8.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(marfil(palette::MARFIL_ALPHA_22))
                        .child(
                            div()
                                .font_family(crate::fonts::FONT_MONO)
                                .child(p.tool.clone()),
                        )
                        .child(div().text_size(px(12.0)).child(p.description.clone()))
                        .children(p.flagged.then(|| {
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(palette::ALAMO))
                                .child(t!("cells.card.flagged").to_string())
                        }))
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new(("perm-approve", i))
                                        .primary()
                                        .label(t!("cells.action.approve").to_string())
                                        .on_click(move |_, _, cx| {
                                            w1.update(cx, |t, cx| t.decide(&s1, &id1, true, cx))
                                                .ok();
                                        }),
                                )
                                .child(
                                    Button::new(("perm-deny", i))
                                        .label(t!("cells.action.deny").to_string())
                                        .on_click(move |_, _, cx| {
                                            w2.update(cx, |t, cx| t.decide(&s2, &id2, false, cx))
                                                .ok();
                                        }),
                                ),
                        ),
                );
            }
        }
        let talk_enabled = card.talk_enabled;
        let buttons = card
            .actions
            .iter()
            .filter(|a| !matches!(a, CellAction::Approve | CellAction::Deny))
            .enumerate()
            .map(|(i, a)| {
                let weak = weak.clone();
                let action = *a;
                let key = match a {
                    CellAction::Cancel => "cancel",
                    CellAction::Delete => "delete",
                    CellAction::Relaunch => "relaunch",
                    CellAction::Talk => "talk",
                    CellAction::Approve => "approve",
                    CellAction::Deny => "deny",
                };
                let b = Button::new(("cell-action", i)).label(tr(&format!("cells.action.{key}")));
                let b = if action == CellAction::Talk {
                    let b = b.primary();
                    if talk_enabled {
                        b
                    } else {
                        b.disabled(true)
                            .tooltip(t!("cells.action.talk_no_session").to_string())
                    }
                } else {
                    b
                };
                b.on_click(move |_, window, cx| {
                    weak.update(cx, |t, cx| t.perform(action, window, cx)).ok();
                })
            });
        col = col
            .child(div().h(px(8.0)))
            .child(h_flex().gap_2().flex_wrap().children(buttons));
        if let Some(n) = &self.notice {
            col = col.child(
                div()
                    .mt(px(8.0))
                    .text_size(px(12.0))
                    .text_color(rgb(palette::BERMELLON))
                    .child(n.clone()),
            );
        }
        col
    }

    fn form_view(&self, f: &FormState, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let engines = self
            .engines()
            .into_iter()
            .enumerate()
            .map(|(i, name)| {
                let weak = weak.clone();
                let n = name.clone();
                Button::new(("engine", i))
                    .label(name.clone())
                    .selected(f.engine == name)
                    .on_click(move |_, _, cx| {
                        let n = n.clone();
                        weak.update(cx, |t, cx| {
                            if let Some(f) = &mut t.form {
                                f.engine = n;
                            }
                            cx.notify();
                        })
                        .ok();
                    })
            })
            .collect::<Vec<_>>();
        let (w_ok, w_cancel) = (weak.clone(), weak);
        v_flex()
            .id("new-agent-form")
            .gap(px(8.0))
            .p(px(16.0))
            .w(px(320.0))
            .h_full()
            .flex_shrink_0()
            .overflow_y_scroll()
            .border_l_1()
            .border_color(marfil(palette::MARFIL_ALPHA_12))
            .child(
                div()
                    .text_size(px(18.0))
                    .child(t!("cells.new.title").to_string()),
            )
            .child(heading(t!("cells.new.prompt").to_string()))
            .child(Input::new(&f.prompt))
            .child(heading(t!("cells.new.engine").to_string()))
            .child(h_flex().gap_1().flex_wrap().children(engines))
            .child(heading(t!("cells.new.model").to_string()))
            .child(Input::new(&f.model))
            .child(heading(t!("cells.new.work_dir").to_string()))
            .child(Input::new(&f.work_dir))
            .child(heading(t!("cells.new.tags").to_string()))
            .child(Input::new(&f.tags))
            .children(self.notice.as_ref().map(|n| {
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(palette::BERMELLON))
                    .child(n.clone())
            }))
            .child(
                h_flex()
                    .gap_2()
                    .mt(px(8.0))
                    .child(
                        Button::new("new-agent-create")
                            .primary()
                            .label(t!("cells.new.create").to_string())
                            .on_click(move |_, window, cx| {
                                w_ok.update(cx, |t, cx| t.submit_form(window, cx)).ok();
                            }),
                    )
                    .child(
                        Button::new("new-agent-cancel")
                            .label(t!("cells.new.cancel").to_string())
                            .on_click(move |_, _, cx| {
                                w_cancel
                                    .update(cx, |t, cx| {
                                        t.form = None;
                                        t.notice = None;
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    ),
            )
    }
}

impl Render for OrganismPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let side = if let Some(f) = &self.form {
            Some(self.form_view(f, cx).into_any_element())
        } else {
            self.selected_card()
                .map(|(card, session)| self.card_view(card, session, cx).into_any_element())
        };
        let empty = self.source.cells().is_empty();
        let new_button = Button::new("new-agent")
            .label(t!("cells.new.button").to_string())
            .on_click(move |_, window, cx| {
                weak.update(cx, |t, cx| t.open_form(window, cx)).ok();
            });
        let back = (self.target.cell().is_some()).then(|| {
            let weak = cx.weak_entity();
            Button::new("target-back")
                .ghost()
                .label(t!("cells.action.back").to_string())
                .on_click(move |_, _, cx| {
                    weak.update(cx, |t, cx| t.back_to_primary(cx)).ok();
                })
        });
        let shown = self
            .organism
            .read(cx)
            .core()
            .borrow()
            .organism
            .shown_state();
        let listening = crate::shell::runtime::is_listening(cx);
        let side = side.or_else(|| {
            (!self.compact).then(|| super::chrome::info_panel(None).into_any_element())
        });
        h_flex()
            .id("organism-pane")
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(|this, _: &BackToPrimary, _, cx| this.back_to_primary(cx)))
            .size_full()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(div().flex_1().min_h_0().child(self.organism.clone()))
                    .child(
                        v_flex()
                            .items_center()
                            .gap(px(8.0))
                            .pb(px(12.0))
                            .child(super::chrome::status(shown))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(super::chrome::mic_button(listening))
                                    .child(new_button)
                                    .children(back),
                            )
                            .children(empty.then(|| {
                                div()
                                    .text_size(px(12.0))
                                    .text_color(marfil(palette::MARFIL_ALPHA_45))
                                    .child(format!(
                                        "{} {}",
                                        t!("cells.empty.title"),
                                        t!("cells.empty.hint")
                                    ))
                            }))
                            .child(super::chrome::legend()),
                    ),
            )
            .children(side)
    }
}

/// A pending approval as the voice layer sees it (PDA-T-0103).
#[derive(Clone, Debug, PartialEq)]
pub struct VoicePending {
    /// Session owning the request.
    pub session: String,
    /// Request id.
    pub id: String,
    /// Cell the session belongs to, when known.
    pub cell_id: Option<String>,
}

/// Voice port: read-only views and voice-origin decisions (PDA-T-0101/0103).
impl OrganismPane {
    /// `(cell id, label, status word, session)` of every cell, for the decision state.
    #[must_use]
    pub fn voice_rows(&self) -> Vec<(String, String, String, Option<String>)> {
        self.source
            .cells()
            .iter()
            .map(|c| {
                let status = self
                    .source
                    .task(c.id.as_str())
                    .map(|t| t.status.clone())
                    .unwrap_or_default();
                (
                    c.id.as_str().to_owned(),
                    c.label.to_string(),
                    status,
                    self.session_of(&c.id),
                )
            })
            .collect()
    }

    /// Every unresolved request in every session ledger.
    #[must_use]
    pub fn voice_pending(&self) -> Vec<VoicePending> {
        let mut out = Vec::new();
        for (session, ledger) in &self.approvals {
            let cell_id = self
                .source
                .cells()
                .iter()
                .find(|c| self.session_of(&c.id).as_deref() == Some(session.as_str()))
                .map(|c| c.id.as_str().to_owned());
            for id in ledger.pending_ids() {
                out.push(VoicePending {
                    session: session.clone(),
                    id,
                    cell_id: cell_id.clone(),
                });
            }
        }
        out
    }

    /// The ledger of a session (decisions made through it must use `Origin::Voice`).
    pub fn voice_ledger(&mut self, session: &str) -> Option<&mut Approvals> {
        self.approvals.get_mut(session)
    }

    /// Delivers whatever the ledgers resolved (call after a voice decision).
    pub fn voice_deliver(&mut self, cx: &mut Context<Self>) {
        self.deliver(cx);
        cx.notify();
    }

    /// The Pando client while connected.
    #[must_use]
    pub fn voice_client(&self) -> Option<PandoClient> {
        self.client()
    }

    /// The session the next voice prompt goes to: the targeted cell's, else the primary one.
    #[must_use]
    pub fn voice_chat_session(&self) -> Option<String> {
        self.target
            .session()
            .map(str::to_owned)
            .or_else(|| self.primary_session.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
    use organism::{BodyState, Engine, Status};
    use pando_client::ConnectionState;
    use pando_client::api::TaskResponse;
    use pando_client::chat::PermissionRequest;

    fn task(id: &str, agent: &str, status: &str) -> TaskResponse {
        TaskResponse {
            id: id.into(),
            name: format!("task {id}"),
            agent: agent.into(),
            status: status.into(),
            created_at: id.into(),
            updated_at: "2026-10-12T07:00:00Z".into(),
            acp_session_id: Some(format!("s-{id}")),
            ..TaskResponse::default()
        }
    }

    fn poll(tasks: Vec<TaskResponse>, perms: Vec<PermissionRequest>) -> Poll {
        Poll {
            tasks,
            permissions: perms,
            now: 1_791_788_400.0,
            ..Poll::default()
        }
    }

    fn pane(
        cx: &mut TestAppContext,
    ) -> (
        Entity<OrganismPane>,
        tokio::runtime::Runtime,
        VisualTestContext,
    ) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("runtime");
        cx.update(|cx| {
            crate::ui::init(cx);
            crate::ui::kit_theme::install(cx);
        });
        let handle = runtime.handle().clone();
        let (_tx, rx) = watch::channel(Snapshot {
            state: ConnectionState::Connecting,
            generation: 0,
            client: None,
        });
        let window = cx.add_window(move |window, cx| {
            OrganismPane::new(window, cx, &handle, rx, CustomEngines::none(), false)
        });
        let view = window.root(cx).expect("root");
        let vcx = VisualTestContext::from_window(window.into(), cx);
        (view, runtime, vcx)
    }

    #[gpui_test]
    fn polls_become_cells_and_new_tasks_delegate(cx: &mut TestAppContext) {
        let (pane, runtime, mut cx) = pane(cx);
        pane.update(&mut cx, |p, cx| {
            p.apply_poll(&poll(vec![task("1", "pando", "running")], vec![]), cx);
        });
        let core = pane.read_with(&cx, |p, cx| p.organism().read(cx).core());
        assert_eq!(core.borrow().cells.len(), 1);
        assert_ne!(core.borrow().organism.shown_state(), BodyState::Delegating);
        pane.update(&mut cx, |p, cx| {
            p.apply_poll(
                &poll(
                    vec![
                        task("1", "pando", "running"),
                        task("2", "acp-claude", "running"),
                    ],
                    vec![],
                ),
                cx,
            );
        });
        assert_eq!(core.borrow().cells.len(), 2);
        assert_eq!(core.borrow().cells[1].engine, Engine::ClaudeAcp);
        assert!(core.borrow().cells[1].is_new);
        assert_eq!(core.borrow().organism.shown_state(), BodyState::Delegating);
        runtime.shutdown_background();
    }

    #[gpui_test]
    fn pending_permission_needs_an_explicit_decision_and_hablarle_sets_the_target(
        cx: &mut TestAppContext,
    ) {
        let (pane, runtime, mut cx) = pane(cx);
        let perm = PermissionRequest {
            id: "p1".into(),
            session_id: "s-1".into(),
            tool_name: "bash".into(),
            require_explicit_approval: true,
            ..PermissionRequest::default()
        };
        let mut t = task("1", "acp-claude", "running");
        t.acp_mode = Some("permission".into());
        pane.update(&mut cx, |p, cx| {
            p.apply_poll(&poll(vec![t.clone()], vec![perm.clone()]), cx);
            p.selected = Some(CellId::new("1"));
        });
        pane.read_with(&cx, |p, _| {
            assert!(p.has_pending());
            let (card, session) = p.selected_card().expect("card");
            assert_eq!(card.status, Status::Waiting);
            assert_eq!(card.pending.len(), 1);
            assert_eq!(session.as_deref(), Some("s-1"));
        });
        // Hablarle routes to the task's session; Esc returns to the primary.
        pane.update(&mut cx, |p, cx| {
            p.talk(&CellId::new("1"), cx);
            assert_eq!(p.target().session(), Some("s-1"));
            assert_eq!(
                p.target_label(),
                t!("cells.target.cell", label = "task 1").to_string()
            );
            p.back_to_primary(cx);
            assert!(p.target().cell().is_none());
        });
        // Deny resolves it; the poll that still lists it cannot revive it.
        pane.update(&mut cx, |p, cx| {
            p.decide("s-1", "p1", false, cx);
            assert!(!p.has_pending());
            p.apply_poll(&poll(vec![t.clone()], vec![perm.clone()]), cx);
            assert!(!p.has_pending(), "a resolved request stays resolved");
        });
        runtime.shutdown_background();
    }

    #[gpui_test]
    fn a_request_the_server_dropped_is_denied_not_left_pending(cx: &mut TestAppContext) {
        let (pane, runtime, mut cx) = pane(cx);
        let perm = PermissionRequest {
            id: "p9".into(),
            session_id: "sess".into(),
            ..PermissionRequest::default()
        };
        pane.update(&mut cx, |p, cx| {
            p.apply_poll(&poll(vec![], vec![perm]), cx);
            assert!(p.has_pending());
            p.apply_poll(&poll(vec![], vec![]), cx);
            assert!(!p.has_pending());
        });
        runtime.shutdown_background();
    }

    #[gpui_test]
    fn cli_cells_refuse_hablarle(cx: &mut TestAppContext) {
        let (pane, runtime, mut cx) = pane(cx);
        pane.update(&mut cx, |p, cx| {
            p.apply_poll(&poll(vec![task("1", "claude", "running")], vec![]), cx);
            p.talk(&CellId::new("1"), cx);
            assert!(p.target().cell().is_none());
            assert!(p.notice.is_some());
        });
        runtime.shutdown_background();
    }
}
