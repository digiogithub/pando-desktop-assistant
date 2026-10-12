//! The voice orchestrator entity (PDA-T-0113/0114): connects the dock (capture, STT), the
//! decision layer, commands, the chat run and the speech queue, following the pure
//! [`Session`] state machine. It owns no logic of its own beyond translating between the
//! machine's inputs/outputs and the real stages.
//!
//! Safety: approvals only ever go through the pane's fail-closed ledger with `Origin::Voice`
//! (the ledger refuses flagged requests); destructive commands need a spoken yes first.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use decide::SystemOneClient;
use decide::command::{Capabilities, Dispatched, dispatch};
use decide::log::DecisionLog;
use decide::policy::{ControlRequest, Outcome, Thresholds};
use decide::questions::AgentTarget;
use decide::questions::ProjectChoice;
use pando_client::PandoClient;
use pando_client::approvals::Resolution;
use pando_client::chat::ChatEvent;
use pando_client::events::{DecisionModelConfig, RouterHealth};
use pando_client::stream::EventStreamConfig;
use rust_i18n::t;
use speech::{Lang, Priority, Utterance};
use voice_proto::Role;

use super::controller::{ConfirmChoice, ConfirmUi, VoiceController, VoiceControllerEvent};
use super::dock::{ActionCard, ResultTone};
use super::latency::{Stage, Turn};
use super::session::{Decided, Input, Output, Session};
use super::speak::{AnswerAccumulator, Fed, speakable};
use crate::cells::pane::OrganismPane;
use crate::decision::confirm::{Confirmation, Verdict, YesNo, lexical_yes_no};
use crate::decision::directory::AgentDirectory;
use crate::decision::engine::{self, Resolved, StateSnapshot};
use crate::decision::executor::{ApprovalScope, PandoExecutor, UiPort, VoiceDecisionReport};
use crate::decision::health::DecisionState;
use crate::decision::speech as cmd_speech;
use crate::decision::spoken_approvals::{self, Applied, Spoken};
use crate::decision::transport::HttpDecisionTransport;
use crate::events::EventPump;
use crate::tokio_bridge;
use crate::ui::{Context, Entity, EventEmitter, Subscription, Task};

/// What the assistant tells the rest of the app.
#[derive(Debug, Clone, PartialEq)]
pub enum AssistantEvent {
    /// Text to insert into the composer (never sent).
    Dictate(String),
    /// A voice run created or continued this chat session: show it.
    ChatSession(String),
    /// The header pill `proyecto · <name> · <p>` changed.
    Project(Option<(String, f64)>),
}

/// Calls from the executor to the UI thread.
#[derive(Debug)]
pub enum UiCall {
    /// Select a cell.
    Open(Option<String>),
    /// Approve / deny pending requests with `Origin::Voice`.
    Decide {
        /// Which requests.
        scope: ApprovalScope,
        /// Approve (true) or deny.
        approve: bool,
        /// Where the report goes.
        reply: async_channel::Sender<VoiceDecisionReport>,
    },
}

/// The executor's bridge to the UI thread.
#[derive(Debug, Clone)]
pub struct ChannelUi {
    tx: async_channel::Sender<Msg>,
}

impl UiPort for ChannelUi {
    fn open(&self, cell_id: Option<String>) {
        let _ = self.tx.try_send(Msg::Ui(UiCall::Open(cell_id)));
    }

    async fn decide(&self, scope: ApprovalScope, approve: bool) -> VoiceDecisionReport {
        let (reply, rx) = async_channel::bounded(1);
        if self
            .tx
            .send(Msg::Ui(UiCall::Decide {
                scope,
                approve,
                reply,
            }))
            .await
            .is_err()
        {
            return VoiceDecisionReport::default();
        }
        rx.recv().await.unwrap_or_default()
    }
}

/// Messages from background tasks.
#[derive(Debug)]
pub enum Msg {
    Resolved {
        resolved: Box<Resolved>,
        directory: AgentDirectory,
    },
    CommandDone {
        speech: String,
        ok: bool,
        record: Box<decide::log::DecisionRecord>,
        line: String,
    },
    Chat(Box<ChatEvent>),
    ChatClosed {
        saw_done: bool,
        failed: bool,
    },
    Ui(UiCall),
    Config(Box<DecisionModelConfig>),
    Health(Box<RouterHealth>),
    Projects(Vec<String>),
    ConfirmExpired(u64),
    SynthCheck(u64),
}

struct SpeakJob {
    utterance: u64,
    remaining: Vec<u64>,
    all_sent: bool,
}

/// The orchestrator.
#[allow(missing_debug_implementations)]
pub struct Assistant {
    voice: Entity<VoiceController>,
    pane: Entity<OrganismPane>,
    tx: async_channel::Sender<Msg>,
    session: Session,
    decision: DecisionState,
    transport: Option<(String, Arc<HttpDecisionTransport>)>,
    confirmation: Option<Confirmation>,
    confirmation_gen: u64,
    awaiting: Option<(String, String)>,
    pill: Option<(String, f64)>,
    projects: Vec<String>,
    log: DecisionLog,
    thresholds: Thresholds,
    chat_session: Option<String>,
    answer: AnswerAccumulator,
    chat_active: bool,
    job: Option<SpeakJob>,
    pending_text: Option<String>,
    turn: Option<Turn>,
    watching: Option<String>,
    claude: Option<crate::ui::WeakEntity<crate::claude::ClaudeBridge>>,
    ticks: u64,
    _subs: Vec<Subscription>,
    _pump: Task<()>,
    _ticker: Task<()>,
}

impl EventEmitter<AssistantEvent> for Assistant {}

impl Assistant {
    /// Builds the orchestrator and starts listening to the dock.
    pub fn new(
        cx: &mut Context<Self>,
        voice: Entity<VoiceController>,
        pane: Entity<OrganismPane>,
        data_dir: PathBuf,
    ) -> Self {
        let (tx, rx) = async_channel::unbounded();
        let pump = EventPump::attach(cx, rx, |this: &mut Self, msg, cx| this.apply(msg, cx));
        let sub = cx.subscribe(&voice, |this, _, ev: &VoiceControllerEvent, cx| {
            this.on_voice(ev, cx)
        });
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            voice,
            pane,
            tx,
            session: Session::new(),
            decision: DecisionState::default(),
            transport: None,
            confirmation: None,
            confirmation_gen: 0,
            awaiting: None,
            pill: None,
            projects: Vec::new(),
            log: DecisionLog::in_dir(&data_dir),
            thresholds: Thresholds::default(),
            chat_session: None,
            answer: AnswerAccumulator::new(),
            chat_active: false,
            job: None,
            pending_text: None,
            turn: None,
            watching: None,
            claude: None,
            ticks: 0,
            _subs: vec![sub],
            _pump: pump,
            _ticker: ticker,
        }
    }

    /// Mute / unmute the assistant's voice (tray, shell): muting stops what is playing and the
    /// text stays on screen.
    pub fn set_muted(&mut self, muted: bool, cx: &mut Context<Self>) {
        let actions = self.session.queue_mut().set_muted(muted);
        let outs = actions
            .into_iter()
            .map(|a| match a {
                speech::Action::Speak(u) => Output::Speak(u),
                speech::Action::Stop { .. } => Output::StopAudio,
            })
            .collect();
        self.run_outputs(outs, cx);
    }

    /// Connects the Claude Code bridge ("dile a <sesión> ...").
    pub fn set_claude(&mut self, bridge: crate::ui::WeakEntity<crate::claude::ClaudeBridge>) {
        self.claude = Some(bridge);
    }

    /// The `decisión · <model>` chip text.
    #[must_use]
    pub fn decision_chip(&self) -> String {
        self.decision.chip_label()
    }

    /// The project pill `proyecto · <name> · <p>`.
    #[must_use]
    pub fn project_pill(&self) -> Option<String> {
        self.pill.as_ref().map(|(n, p)| {
            t!(
                "voice.pill.project",
                name = n,
                p = format!("{:.0}", p * 100.0)
            )
            .to_string()
        })
    }

    /// The session stage (tests, debug panel).
    #[must_use]
    pub fn stage(&self) -> super::session::Stage {
        self.session.stage()
    }

    /// Queues something to say when the user is not speaking (Claude Stop hook, conclusions).
    pub fn notify(
        &mut self,
        text: String,
        priority: Priority,
        source: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let outs = self.session.step(Input::Notify {
            text,
            priority,
            source,
        });
        self.run_outputs(outs, cx);
    }

    // ------------------------------------------------------------------ plumbing

    fn now(&self, cx: &Context<Self>) -> u64 {
        self.voice.read(cx).now_ms()
    }

    fn feed(&mut self, input: Input, cx: &mut Context<Self>) {
        let outs = self.session.step(input);
        self.run_outputs(outs, cx);
    }

    fn run_outputs(&mut self, outs: Vec<Output>, cx: &mut Context<Self>) {
        for out in outs {
            match out {
                Output::SetBody(_) => {
                    // The dock owns the body state while listening; afterwards the machine's
                    // stage decides (see `sync_dock`).
                    self.sync_dock(cx);
                }
                Output::StopAudio => self.stop_audio(cx),
                Output::Speak(u) => self.speak(u, cx),
                Output::Decide(text) => self.decide(text, cx),
                Output::Chat { text, steer } => self.start_chat(text, steer, cx),
                Output::CancelRun => self.cancel_run(cx),
                Output::ShowText(text) => {
                    self.voice.update(cx, |v, cx| {
                        v.set_notice_text(text, cx);
                    });
                }
            }
        }
    }

    fn sync_dock(&mut self, cx: &mut Context<Self>) {
        let body = self.session.body();
        self.voice.update(cx, |v, cx| v.set_body(body, cx));
    }

    // ------------------------------------------------------------------ voice events

    fn on_voice(&mut self, ev: &VoiceControllerEvent, cx: &mut Context<Self>) {
        match ev {
            VoiceControllerEvent::Pressed => self.feed(Input::PttPressed, cx),
            VoiceControllerEvent::Released { submitted } => {
                self.feed(
                    Input::PttReleased {
                        submitted: *submitted,
                    },
                    cx,
                );
            }
            VoiceControllerEvent::Confirm(choice) => self.on_confirm_click(*choice, cx),
            VoiceControllerEvent::HandsFreeBargeIn => {
                self.feed(Input::BargeIn, cx);
            }
            VoiceControllerEvent::BargeIn => {
                // The service already stopped playback; drop the synthesis of the interrupted utterance.
                self.cancel_job(cx);
            }
            VoiceControllerEvent::Transcript { text, turn, .. } => {
                self.turn = Some(turn.clone());
                self.pending_text = Some(text.clone());
                self.feed(Input::Transcript(text.clone()), cx);
            }
            VoiceControllerEvent::SynthFinished { id, failed } => {
                self.synth_finished(*id, *failed, cx)
            }
            VoiceControllerEvent::PlaybackStarted => {
                if let Some(job) = &self.job {
                    let id = job.utterance;
                    self.feed(Input::SpeechStarted(id), cx);
                }
            }
            VoiceControllerEvent::PlaybackDrained => self.maybe_finish_job(cx),
            VoiceControllerEvent::WorkersChanged => {
                let down = !self.voice.read(cx).dock.stt.is_ready();
                if down {
                    self.feed(Input::SttFailed, cx);
                }
            }
        }
    }

    // ------------------------------------------------------------------ decision

    fn snapshot(&self, cx: &Context<Self>) -> StateSnapshot {
        let pane = self.pane.read(cx);
        let directory = AgentDirectory::from_rows(pane.voice_rows());
        let pending = pane
            .voice_pending()
            .into_iter()
            .filter_map(|p| {
                let cell = p.cell_id?;
                let entry = directory.entries().iter().find(|e| e.cell_id == cell)?;
                Some(decide::policy::PendingApproval {
                    agent_id: entry.slug.clone(),
                    explicit_only: self.is_flagged(&p.session, &p.id, cx),
                })
            })
            .collect();
        StateSnapshot {
            agents: directory,
            projects: self.projects.clone(),
            current_project: self.pill.as_ref().map(|(n, _)| n.clone()),
            pending,
        }
    }

    fn is_flagged(&self, session: &str, id: &str, cx: &Context<Self>) -> bool {
        self.pane.read(cx).voice_is_flagged(session, id)
    }

    fn decide(&mut self, text: String, cx: &mut Context<Self>) {
        let now = self.now(cx);
        // 1. A spoken answer to an open confirmation.
        if let Some(c) = self.confirmation.clone() {
            let answer = lexical_yes_no(&text);
            match c.answer(answer, now) {
                Verdict::Execute(request) => {
                    self.confirmation = None;
                    self.voice.update(cx, |v, cx| v.set_confirm_ui(None, cx));
                    self.feed(Input::Decided(Decided::Command), cx);
                    self.run_command(request, c.transcript.clone(), now, cx);
                    return;
                }
                Verdict::Dismissed if answer == Some(YesNo::No) => {
                    self.confirmation = None;
                    self.voice.update(cx, |v, cx| v.set_confirm_ui(None, cx));
                    self.feed(Input::ConfirmationClosed { run: false }, cx);
                    // The machine is still Deciding: settle it.
                    self.feed(Input::Decided(Decided::ExplicitOnly), cx);
                    let speech = t!("voice.confirm.dismissed").to_string();
                    self.notify(speech, Priority::Direct, None, cx);
                    return;
                }
                // Anything else is a new sentence: the confirmation lapses, nothing runs.
                _ => {
                    self.confirmation = None;
                    self.voice.update(cx, |v, cx| v.set_confirm_ui(None, cx));
                }
            }
        }
        // 2. A spoken answer to a read-aloud approval.
        if let Some((session, id)) = self.awaiting.clone() {
            let n_options = self.option_count(&session, &id, cx);
            let spoken = spoken_approvals::interpret(&text, n_options);
            if spoken != Spoken::Unclear {
                self.awaiting = None;
                let applied = self.pane.update(cx, |p, cx| {
                    let applied = p
                        .voice_ledger(&session)
                        .map_or(Applied::Gone, |l| spoken_approvals::apply(l, &id, &spoken));
                    p.voice_deliver(cx);
                    applied
                });
                self.feed(Input::Decided(Decided::ExplicitOnly), cx);
                let speech = match applied {
                    Applied::Resolved(Resolution::Approved | Resolution::ApprovedForSession) => {
                        t!("voice.cmd.done").to_string()
                    }
                    Applied::Resolved(_) => t!("voice.cmd.done").to_string(),
                    Applied::NeedsScreen => t!("voice.cmd.needs_screen").to_string(),
                    Applied::Gone | Applied::Unclear => t!("voice.cmd.nothing_pending").to_string(),
                };
                self.notify(speech, Priority::Direct, None, cx);
                return;
            }
            self.awaiting = None;
        }
        // 2b. "dile a <sesión> que ...": a message for a Claude Code session.
        if let Some(tell) = crate::claude::tell::parse(&text)
            && let Some(bridge) = self
                .claude
                .as_ref()
                .and_then(crate::ui::WeakEntity::upgrade)
        {
            let found = bridge.read(cx).find_session(&tell.target);
            self.feed(Input::Decided(Decided::ExplicitOnly), cx);
            match found {
                Some(cell) => {
                    bridge.update(cx, |b, cx| b.send(&cell.session_id, tell.body.clone(), cx));
                }
                None => self.notify(
                    t!("claude.not_found").to_string(),
                    Priority::Direct,
                    None,
                    cx,
                ),
            }
            return;
        }
        // 3. The decision model (fail-open).
        let snapshot = self.snapshot(cx);
        let endpoint = self.decision.endpoint(|k| std::env::var(k).ok());
        let try_it = self.decision.should_try();
        if !try_it || endpoint.is_none() {
            let resolved = Resolved {
                outcome: Outcome::unavailable(),
                record: decide::log::DecisionRecord::new(
                    now,
                    &text,
                    &Err(decide::cascade::CascadeFailure {
                        error: decide::DecisionError::Unreachable(
                            "decision model not available".into(),
                        ),
                        exchanges: Vec::new(),
                    }),
                    Outcome::unavailable(),
                    false,
                ),
                failed: true,
                latency_ms: 0,
            };
            let _ = self.tx.try_send(Msg::Resolved {
                resolved: Box::new(resolved),
                directory: snapshot.agents,
            });
            return;
        }
        let Some(endpoint) = endpoint else { return };
        let key = endpoint.url.clone();
        let transport = match &self.transport {
            Some((k, t)) if *k == key => t.clone(),
            _ => match HttpDecisionTransport::new(endpoint.clone()) {
                Ok(t) => {
                    let t = Arc::new(t);
                    self.transport = Some((key, t.clone()));
                    t
                }
                Err(e) => {
                    tracing::warn!("decision transport: {e}");
                    return;
                }
            },
        };
        let (tx, thresholds) = (self.tx.clone(), self.thresholds);
        let options = endpoint.options();
        let directory = snapshot.agents.clone();
        tokio_bridge::spawn(cx, async move {
            let client = SystemOneClient::new(transport, options);
            let resolved =
                engine::decide_transcript(&client, &text, &snapshot, thresholds, now).await;
            let _ = tx
                .send(Msg::Resolved {
                    resolved: Box::new(resolved),
                    directory,
                })
                .await;
        })
        .detach();
    }

    fn option_count(&self, session: &str, id: &str, cx: &Context<Self>) -> usize {
        self.pane.read(cx).voice_option_count(session, id)
    }

    fn on_resolved(
        &mut self,
        resolved: Resolved,
        directory: AgentDirectory,
        cx: &mut Context<Self>,
    ) {
        let now = self.now(cx);
        self.decision.record_call(resolved.failed);
        if let Some(mut turn) = self.turn.take() {
            turn.mark(Stage::Decision, now);
            self.turn = Some(turn);
        }
        let text = resolved.record.transcript.clone();
        let card: ActionCard = engine::card_from(&resolved.record);
        self.voice.update(cx, |v, cx| v.set_card(card, cx));
        match resolved.outcome.clone() {
            Outcome::Execute { request } => {
                self.feed(Input::Decided(Decided::Command), cx);
                self.run_command(request, text, now, cx);
            }
            Outcome::Confirm { request, reason } => {
                let prompt = cmd_speech::confirm_prompt(&directory, &request, reason);
                self.confirmation = Some(Confirmation::confirm(&text, request, reason, now));
                self.voice.update(cx, |v, cx| {
                    v.set_confirm_ui(
                        Some(ConfirmUi {
                            prompt: prompt.clone(),
                            options: Vec::new(),
                            yes_no: true,
                        }),
                        cx,
                    );
                });
                self.confirmation_gen += 1;
                let generation = self.confirmation_gen;
                self.log_record(&resolved.record, false);
                self.feed(Input::Decided(Decided::Confirm), cx);
                self.notify(prompt, Priority::Direct, None, cx);
                let tx = self.tx.clone();
                tokio_bridge::spawn(cx, async move {
                    tokio::time::sleep(crate::decision::confirm::CONFIRM_TIMEOUT).await;
                    let _ = tx.send(Msg::ConfirmExpired(generation)).await;
                })
                .detach();
            }
            Outcome::ExplicitOnly { request } => {
                self.log_record(&resolved.record, false);
                self.feed(Input::Decided(Decided::ExplicitOnly), cx);
                if let AgentTarget::Agent(slug) = &request.target
                    && let Some(e) = directory.resolve(slug)
                {
                    let cell = e.cell_id.clone();
                    self.pane.update(cx, |p, cx| p.select_cell(&cell, cx));
                }
                self.notify(
                    t!("voice.confirm.needs_screen").to_string(),
                    Priority::Direct,
                    None,
                    cx,
                );
            }
            Outcome::SwitchProject { project, p } => {
                self.log_record(&resolved.record, true);
                let name = match project {
                    ProjectChoice::Named(n) => Some(n),
                    ProjectChoice::Current => None,
                };
                if let Some(name) = name {
                    self.pill = Some((name.clone(), p));
                    cx.emit(AssistantEvent::Project(self.pill.clone()));
                    self.feed(Input::Decided(Decided::SwitchProject), cx);
                    self.notify(
                        t!("voice.pill.switched", name = name).to_string(),
                        Priority::Direct,
                        None,
                        cx,
                    );
                } else {
                    self.feed(Input::Decided(Decided::SwitchProject), cx);
                }
            }
            Outcome::Dictate { text } => {
                self.log_record(&resolved.record, true);
                self.feed(Input::Decided(Decided::Dictate), cx);
                cx.emit(AssistantEvent::Dictate(text));
            }
            Outcome::Conversation {
                reason: decide::policy::ConversationReason::LowConfidence,
                top2,
            } if !top2.is_empty() => {
                self.log_record(&resolved.record, false);
                let mut options: Vec<(String, f64)> = top2;
                options.truncate(2);
                let mut labels: Vec<String> =
                    options.iter().map(|(k, _)| k.replace('_', " ")).collect();
                labels.push(t!("voice.confirm.chat").to_string());
                self.confirmation = Some(Confirmation::pick(&text, options, now));
                self.confirmation_gen += 1;
                let generation = self.confirmation_gen;
                self.voice.update(cx, |v, cx| {
                    v.set_confirm_ui(
                        Some(ConfirmUi {
                            prompt: t!("voice.confirm.pick_prompt").to_string(),
                            options: labels,
                            yes_no: false,
                        }),
                        cx,
                    );
                });
                self.pending_text = Some(text);
                self.feed(Input::Decided(Decided::Confirm), cx);
                let tx = self.tx.clone();
                tokio_bridge::spawn(cx, async move {
                    tokio::time::sleep(crate::decision::confirm::CONFIRM_TIMEOUT).await;
                    let _ = tx.send(Msg::ConfirmExpired(generation)).await;
                })
                .detach();
            }
            Outcome::Conversation { .. } => {
                self.log_record(&resolved.record, false);
                self.feed(Input::Decided(Decided::Conversation), cx);
                let steer = self.session.run_active() || self.chat_active;
                let out = self.session.chat_output(text);
                let _ = steer;
                self.run_outputs(vec![out], cx);
            }
        }
    }

    /// A click on the confirmation buttons: the same verdicts as a spoken answer.
    fn on_confirm_click(&mut self, choice: ConfirmChoice, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let Some(c) = self.confirmation.clone() else {
            return;
        };
        self.confirmation = None;
        self.voice.update(cx, |v, cx| v.set_confirm_ui(None, cx));
        // The last button of a choice is "Hablar con Pando".
        if let (ConfirmChoice::Pick(i), crate::decision::confirm::Pending::Pick { options }) =
            (choice, &c.pending)
            && i >= options.len()
            && !c.expired(now)
        {
            self.feed(Input::ConfirmationClosed { run: false }, cx);
            self.feed(Input::Transcript(c.transcript.clone()), cx);
            self.feed(Input::Decided(Decided::Conversation), cx);
            let out = self.session.chat_output(c.transcript);
            self.run_outputs(vec![out], cx);
            return;
        }
        let verdict = match choice {
            ConfirmChoice::Yes => c.answer(Some(YesNo::Yes), now),
            ConfirmChoice::No => c.answer(Some(YesNo::No), now),
            ConfirmChoice::Pick(i) => c.choose(i, now),
        };
        match verdict {
            Verdict::Execute(request) => {
                self.feed(Input::ConfirmationClosed { run: true }, cx);
                self.run_command(request, c.transcript.clone(), now, cx);
            }
            Verdict::Reask(text) => {
                self.feed(Input::ConfirmationClosed { run: false }, cx);
                self.feed(Input::Transcript(text), cx);
            }
            Verdict::Dismissed | Verdict::Waiting => {
                self.feed(Input::ConfirmationClosed { run: false }, cx);
            }
        }
    }

    fn log_record(&self, record: &decide::log::DecisionRecord, executed: bool) {
        let mut r = record.clone();
        r.executed = executed;
        if let Err(e) = self.log.append(&r) {
            tracing::warn!("could not append to decisions.jsonl: {e}");
        }
    }

    fn run_command(
        &mut self,
        request: ControlRequest,
        transcript: String,
        now: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.pane.read(cx).voice_client() else {
            self.feed(
                Input::CommandFinished {
                    ok: false,
                    speech: Some(t!("voice.error.chat").to_string()),
                },
                cx,
            );
            return;
        };
        let snapshot = self.snapshot(cx);
        let directory = snapshot.agents.clone();
        let ui = Arc::new(ChannelUi {
            tx: self.tx.clone(),
        });
        let executor = PandoExecutor::new(Arc::new(client), ui, directory.clone())
            .with_default_project(self.pill.as_ref().map(|(n, _)| n.clone()));
        let tx = self.tx.clone();
        let record_text = transcript.clone();
        let line = engine::describe(&Outcome::Execute {
            request: request.clone(),
        });
        let outcome = Outcome::Execute {
            request: request.clone(),
        };
        tokio_bridge::spawn(cx, async move {
            let dispatched =
                dispatch(&executor, &request, &transcript, Capabilities::default()).await;
            let (ok, speech) = match &dispatched {
                Dispatched::Ran(r) => (r.is_ok(), cmd_speech::command_speech(&directory, r)),
                Dispatched::Refused(refusal) => (false, cmd_speech::refusal_speech(refusal)),
            };
            let record = decide::log::DecisionRecord {
                ts_ms: now,
                transcript: record_text,
                state_hash: String::new(),
                model: String::new(),
                exchanges: Vec::new(),
                decision: None,
                error: None,
                latency_ms: 0,
                policy: outcome,
                executed: ok,
            };
            let _ = tx
                .send(Msg::CommandDone {
                    speech,
                    ok,
                    record: Box::new(record),
                    line,
                })
                .await;
        })
        .detach();
    }

    // ------------------------------------------------------------------ messages

    fn apply(&mut self, msg: Msg, cx: &mut Context<Self>) {
        match msg {
            Msg::Resolved {
                resolved,
                directory,
            } => self.on_resolved(*resolved, directory, cx),
            Msg::CommandDone {
                speech,
                ok,
                record,
                line,
            } => {
                let now = self.now(cx);
                if let Err(e) = self.log.append(&record) {
                    tracing::warn!("could not append to decisions.jsonl: {e}");
                }
                let tone = if ok {
                    ResultTone::Ok
                } else {
                    ResultTone::Failed
                };
                self.voice.update(cx, |v, cx| {
                    v.set_result(format!("{line} · {speech}"), tone, cx)
                });
                if let Some(mut turn) = self.turn.take() {
                    turn.mark(Stage::Api, now);
                    self.voice.update(cx, |v, _| v.finish_turn(&turn));
                }
                self.feed(
                    Input::CommandFinished {
                        ok,
                        speech: Some(speech),
                    },
                    cx,
                );
            }
            Msg::Chat(ev) => self.on_chat(*ev, cx),
            Msg::ChatClosed { saw_done, failed } => {
                self.chat_active = false;
                if failed {
                    self.feed(Input::ChatFailed, cx);
                    self.notify(
                        t!("voice.error.chat").to_string(),
                        Priority::Direct,
                        None,
                        cx,
                    );
                } else if !saw_done {
                    self.feed(Input::ChatDetached, cx);
                }
            }
            Msg::Ui(call) => self.on_ui(call, cx),
            Msg::Config(c) => self.decision.set_config(*c),
            Msg::Health(h) => self.decision.set_health(*h),
            Msg::Projects(p) => self.projects = p,
            Msg::ConfirmExpired(generation) => {
                if generation == self.confirmation_gen && self.confirmation.take().is_some() {
                    self.voice.update(cx, |v, cx| v.set_confirm_ui(None, cx));
                    self.feed(Input::ConfirmationClosed { run: false }, cx);
                }
            }
            Msg::SynthCheck(utt) => {
                if self
                    .job
                    .as_ref()
                    .is_some_and(|j| j.utterance == utt && j.all_sent)
                    && !self
                        .voice
                        .read(cx)
                        .service()
                        .is_some_and(super::service::VoiceService::is_speaking)
                {
                    self.finish_job(cx);
                }
            }
        }
    }

    fn on_ui(&mut self, call: UiCall, cx: &mut Context<Self>) {
        match call {
            UiCall::Open(cell) => {
                self.pane.update(cx, |p, cx| match cell {
                    Some(id) => p.select_cell(&id, cx),
                    None => p.back_to_primary(cx),
                });
            }
            UiCall::Decide {
                scope,
                approve,
                reply,
            } => {
                let report = self.pane.update(cx, |p, cx| {
                    let mut report = VoiceDecisionReport::default();
                    let spoken = if approve {
                        Spoken::Approve
                    } else {
                        Spoken::Deny
                    };
                    for pending in p.voice_pending() {
                        let wanted = match &scope {
                            ApprovalScope::All => true,
                            ApprovalScope::Cells(cells) => {
                                pending.cell_id.as_ref().is_some_and(|c| cells.contains(c))
                            }
                        };
                        if !wanted {
                            continue;
                        }
                        let applied = p.voice_ledger(&pending.session).map_or(Applied::Gone, |l| {
                            spoken_approvals::apply(l, &pending.id, &spoken)
                        });
                        match applied {
                            Applied::Resolved(_) => report.resolved += 1,
                            Applied::NeedsScreen => report.needs_explicit += 1,
                            Applied::Gone | Applied::Unclear => {}
                        }
                    }
                    p.voice_deliver(cx);
                    report
                });
                let _ = reply.try_send(report);
            }
        }
    }

    // ------------------------------------------------------------------ chat

    fn start_chat(&mut self, text: String, steer: bool, cx: &mut Context<Self>) {
        let pane = self.pane.read(cx);
        let Some(client) = pane.voice_client() else {
            self.feed(Input::ChatFailed, cx);
            self.notify(
                t!("voice.error.chat").to_string(),
                Priority::Direct,
                None,
                cx,
            );
            return;
        };
        let session = pane
            .voice_chat_session()
            .or_else(|| self.chat_session.clone());
        let tx = self.tx.clone();
        self.answer = AnswerAccumulator::new();
        if steer && let Some(session) = session {
            tokio_bridge::spawn(cx, async move {
                if let Err(e) = client.steer(&session, &text).await {
                    tracing::warn!("steering failed: {e}");
                }
            })
            .detach();
            return;
        }
        self.chat_active = true;
        tokio_bridge::spawn(cx, run_chat(client, session, text, tx)).detach();
    }

    fn cancel_run(&mut self, cx: &mut Context<Self>) {
        let session = self.chat_session.clone();
        let client = self.pane.read(cx).voice_client();
        if let (Some(session), Some(client)) = (session, client) {
            tokio_bridge::spawn(cx, async move {
                let _ = client.cancel(&session).await;
            })
            .detach();
        }
        self.chat_active = false;
    }

    fn on_chat(&mut self, ev: ChatEvent, cx: &mut Context<Self>) {
        let now = self.now(cx);
        if let ChatEvent::Session { session_id, .. } = &ev
            && !session_id.is_empty()
            && self.chat_session.as_deref() != Some(session_id)
        {
            self.chat_session = Some(session_id.clone());
            cx.emit(AssistantEvent::ChatSession(session_id.clone()));
        }
        match &ev {
            ChatEvent::PermissionRequest(_) | ChatEvent::QuestionRequest(_) => {
                self.hint_pending(cx);
            }
            _ => {}
        }
        match self.answer.feed(&ev) {
            Fed::FirstContent => {
                if let Some(mut turn) = self.turn.take() {
                    turn.mark(Stage::Api, now);
                    self.turn = Some(turn);
                }
                self.feed(Input::ChatFirstContent, cx);
            }
            Fed::Done => {
                self.chat_active = false;
                let text = speakable(self.answer.text()).map(|s| s.text);
                if self.voice.read(cx).config().speak {
                    self.feed(Input::ChatFinished(text), cx);
                } else {
                    self.feed(Input::ChatFinished(None), cx);
                }
            }
            Fed::Failed | Fed::Nothing => {}
        }
    }

    /// A permission or question showed up during a voice run: read the oldest pending item
    /// aloud once the ledger has it (the pane's poll registers permissions).
    fn hint_pending(&mut self, cx: &mut Context<Self>) {
        let pending = self.pane.read(cx).voice_pending();
        if let Some(p) = pending.first() {
            self.read_aloud(p.session.clone(), p.id.clone(), cx);
        }
    }

    fn read_aloud(&mut self, session: String, id: String, cx: &mut Context<Self>) {
        if self
            .awaiting
            .as_ref()
            .is_some_and(|(s, i)| *s == session && *i == id)
        {
            return;
        }
        let body = self.pane.update(cx, |p, _| {
            p.voice_ledger(&session)
                .and_then(|l| l.request(&id).cloned())
        });
        if let Some(body) = body {
            self.awaiting = Some((session, id));
            let text = spoken_approvals::read_aloud(&body);
            self.notify(text, Priority::Direct, None, cx);
        }
    }

    // ------------------------------------------------------------------ speech

    fn stop_audio(&mut self, cx: &mut Context<Self>) {
        self.cancel_job(cx);
        self.voice.update(cx, |v, _| {
            if let Some(s) = v.service() {
                s.stop_speaking();
            }
        });
    }

    fn cancel_job(&mut self, cx: &mut Context<Self>) {
        if let Some(job) = self.job.take() {
            let ids = job.remaining;
            self.voice.update(cx, |v, _| {
                if let Some(s) = v.service() {
                    for id in ids {
                        s.cancel_synthesis(id);
                    }
                }
            });
        }
    }

    fn speak(&mut self, u: Utterance, cx: &mut Context<Self>) {
        let lang = Lang::from_code(&self.voice.read(cx).config().language);
        let text = speech::normalize(&u.text, lang);
        let sentences = speech::segments(&text);
        let mut ids = Vec::new();
        let ok = self.voice.update(cx, |v, _| {
            let Some(s) = v.service() else { return false };
            for seg in &sentences {
                match s.synthesize(seg.clone()) {
                    Ok(id) => ids.push(id),
                    Err(_) => return false,
                }
            }
            !ids.is_empty()
        });
        if !ok {
            // No voice: the text stays on screen and the queue moves on.
            self.voice
                .update(cx, |v, cx| v.set_notice_text(u.text.clone(), cx));
            self.feed(Input::SpeechFinished(u.id), cx);
            return;
        }
        self.job = Some(SpeakJob {
            utterance: u.id,
            remaining: ids,
            all_sent: false,
        });
    }

    fn synth_finished(&mut self, id: u64, _failed: bool, cx: &mut Context<Self>) {
        let Some(job) = self.job.as_mut() else { return };
        job.remaining.retain(|r| *r != id);
        if job.remaining.is_empty() {
            job.all_sent = true;
            let utt = job.utterance;
            let tx = self.tx.clone();
            tokio_bridge::spawn(cx, async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                let _ = tx.send(Msg::SynthCheck(utt)).await;
            })
            .detach();
        }
    }

    fn maybe_finish_job(&mut self, cx: &mut Context<Self>) {
        if self.job.as_ref().is_some_and(|j| j.all_sent) {
            self.finish_job(cx);
        }
    }

    fn finish_job(&mut self, cx: &mut Context<Self>) {
        if let Some(job) = self.job.take() {
            let now = self.now(cx);
            if let Some(mut turn) = self.turn.take() {
                turn.mark(Stage::FirstAudio, now);
                self.voice.update(cx, |v, _| v.finish_turn(&turn));
            }
            self.feed(Input::SpeechFinished(job.utterance), cx);
        }
    }

    // ------------------------------------------------------------------ background upkeep

    fn tick(&mut self, cx: &mut Context<Self>) {
        self.ticks += 1;
        let now = self.now(cx);
        if let Some(c) = &self.confirmation
            && c.expired(now)
        {
            // The timer message normally does this; this is the safety net.
            self.confirmation = None;
            self.voice.update(cx, |v, cx| v.set_confirm_ui(None, cx));
            self.feed(Input::ConfirmationClosed { run: false }, cx);
        }
        let Some(client) = self.pane.read(cx).voice_client() else {
            self.watching = None;
            return;
        };
        let base = client.base_url().to_owned();
        if self.watching.as_deref() != Some(base.as_str()) {
            self.watching = Some(base);
            self.start_watcher(client.clone(), cx);
        }
        if self.ticks % 30 == 1 {
            let tx = self.tx.clone();
            let c = client.clone();
            tokio_bridge::spawn(cx, async move {
                if let Ok(projects) = c.list_projects().await {
                    let _ = tx
                        .send(Msg::Projects(
                            projects.into_iter().map(|p| p.name).collect(),
                        ))
                        .await;
                }
            })
            .detach();
        }
        if self.ticks % 60 == 2 {
            let tx = self.tx.clone();
            tokio_bridge::spawn(cx, async move {
                if let Ok(h) = client.decision_model_health().await {
                    let _ = tx.send(Msg::Health(Box::new(h))).await;
                }
            })
            .detach();
        }
    }

    fn start_watcher(&mut self, client: PandoClient, cx: &mut Context<Self>) {
        let tx = self.tx.clone();
        tokio_bridge::spawn(cx, async move {
            let mut watcher = client.watch_decision_model(EventStreamConfig::default());
            while let Some(cfg) = watcher.next().await {
                if let Ok(cfg) = cfg
                    && tx.send(Msg::Config(Box::new(cfg))).await.is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// True when the STT worker is up (for tests of the wiring).
    #[must_use]
    pub fn stt_ready(&self, cx: &Context<Self>) -> bool {
        self.voice
            .read(cx)
            .service()
            .is_some_and(|s| s.worker_state(Role::Stt).is_ready())
    }
}

async fn run_chat(
    client: PandoClient,
    session: Option<String>,
    text: String,
    tx: async_channel::Sender<Msg>,
) {
    let mut stream = match client.chat_stream(session.as_deref(), &text, None).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("chat stream failed to open: {e}");
            let _ = tx
                .send(Msg::ChatClosed {
                    saw_done: false,
                    failed: true,
                })
                .await;
            return;
        }
    };
    let mut saw_done = false;
    loop {
        match stream.next().await {
            Ok(Some(ev)) => {
                let done = matches!(ev, ChatEvent::Done);
                let errored = matches!(ev, ChatEvent::Error(_));
                if tx.send(Msg::Chat(Box::new(ev))).await.is_err() {
                    return;
                }
                if done {
                    saw_done = true;
                    break;
                }
                if errored {
                    let _ = tx
                        .send(Msg::ChatClosed {
                            saw_done: false,
                            failed: true,
                        })
                        .await;
                    return;
                }
            }
            Ok(None) => break,
            Err(e) => {
                tracing::warn!("chat stream error: {e}");
                let _ = tx
                    .send(Msg::ChatClosed {
                        saw_done: false,
                        failed: true,
                    })
                    .await;
                return;
            }
        }
    }
    let _ = tx
        .send(Msg::ChatClosed {
            saw_done,
            failed: false,
        })
        .await;
}

struct AssistantHandle(crate::ui::WeakEntity<Assistant>);

impl crate::ui::Global for AssistantHandle {}

/// Remembers the assistant so shell signals (mute) can reach it.
pub fn register_handle(cx: &mut crate::ui::App, assistant: &Entity<Assistant>) {
    cx.set_global(AssistantHandle(assistant.downgrade()));
}

/// A signal from the shell (tray, hotkeys): hands-free and mute.
pub fn shell_output(cx: &mut crate::ui::App, output: crate::shell::controller::Output) {
    use crate::shell::controller::Output as O;
    match output {
        O::Ptt(edge) => super::controller::ptt_edge(cx, edge),
        O::HandsFree(on) => super::controller::set_hands_free(cx, on),
        O::Muted(on) => {
            if let Some(h) = cx.try_global::<AssistantHandle>().map(|h| h.0.clone()) {
                h.update(cx, |a, cx| a.set_muted(on, cx)).ok();
            }
        }
    }
}
