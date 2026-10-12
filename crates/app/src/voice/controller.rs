//! GPUI controller of the voice dock: owns the [`VoiceService`], applies its events to the
//! [`DockModel`], drives the organism body state and levels, and renders the dock
//! (PDA-T-0085/0090/0094/0095).

use std::sync::Arc;
use std::time::{Duration, Instant};

use organism::BodyState;
use rust_i18n::t;
use voice_proto::{FromWorker, Role};

use super::config::VoiceConfig;
use super::devices::{self, DeviceChoice};
use super::dock::{ActionCard, DockModel, Effect, Notice, Phase, ResultTone};
use super::latency::{Stage, Stats, Turn};
use super::levels::OrganismLevels;
use super::service::{PttEvent, VoiceEvent, VoiceService};
use super::supervisor::{WorkerEvent, WorkerState};
use crate::events::EventPump;
use crate::organism_view::OrganismView;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::canvas::MouseUpEvent;
use crate::ui::paint::{MouseButton, MouseDownEvent};
use crate::ui::theme::palette;
use crate::ui::{
    Context, Entity, EventEmitter, FocusHandle, Focusable, Hsla, InteractiveElement as _,
    IntoElement, KeyDownEvent, KeyUpEvent, ParentElement as _, Render, Styled as _, Task, Window,
    div, h_flex, px, rgb, rgba, v_flex,
};

struct VoiceHandle(crate::ui::WeakEntity<VoiceController>);

impl crate::ui::Global for VoiceHandle {}

/// Remembers the controller so the shell's global push-to-talk can reach it.
pub fn register_handle(cx: &mut crate::ui::App, voice: &Entity<VoiceController>) {
    cx.set_global(VoiceHandle(voice.downgrade()));
}

/// A push-to-talk edge from the shell (global hotkey, `pando-assistant ptt`, tray, mini window).
pub fn ptt_edge(cx: &mut crate::ui::App, edge: crate::shell::ptt::PttEdge) {
    let Some(handle) = cx.try_global::<VoiceHandle>().map(|h| h.0.clone()) else {
        return;
    };
    handle
        .update(cx, |v, cx| match edge {
            crate::shell::ptt::PttEdge::Started => v.press(cx),
            crate::shell::ptt::PttEdge::Stopped => v.release(cx),
        })
        .ok();
}

/// The push-to-talk key (hold).
pub const PTT_KEY: &str = "f9";

/// What the controller tells the rest of the app.
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceControllerEvent {
    /// A final transcript is ready for the decision layer.
    Transcript {
        /// STT request id.
        id: u64,
        /// Text.
        text: String,
        /// Timestamps so far (release, PCM ready, transcript).
        turn: Turn,
    },
    /// The user pressed push-to-talk while the assistant was speaking: stop everything queued.
    BargeIn,
    /// A synthesis request finished or failed (the queue can send the next sentence).
    SynthFinished {
        /// Request id.
        id: u64,
        /// It failed or was cancelled.
        failed: bool,
    },
    /// Worker availability changed.
    WorkersChanged,
    /// Push-to-talk went down and a recording started.
    Pressed,
    /// Push-to-talk went up; `submitted` is false for a tap or when nothing could be sent.
    Released {
        /// Audio was handed to the STT worker.
        submitted: bool,
    },
    /// Playback started.
    PlaybackStarted,
    /// Everything queued has been played (or was stopped).
    PlaybackDrained,
}

/// The dock entity.
#[allow(missing_debug_implementations)]
pub struct VoiceController {
    service: Option<VoiceService>,
    config: VoiceConfig,
    /// The model drawn by [`Render`].
    pub dock: DockModel,
    organism: Option<Entity<OrganismView>>,
    levels: OrganismLevels,
    /// Latency statistics for the debug panel.
    pub stats: Stats,
    turn: Option<Turn>,
    epoch: Instant,
    last_tick: Instant,
    ticker: Option<Task<()>>,
    focus: FocusHandle,
    dirs: VoiceDirs,
    sink: Arc<dyn Fn(VoiceEvent) + Send + Sync>,
    settings_open: bool,
    input_devices: Vec<DeviceChoice>,
    output_devices: Vec<DeviceChoice>,
    _pump: Task<()>,
}

/// Where the voice settings and data live.
#[derive(Debug, Clone, Default)]
pub struct VoiceDirs {
    /// Directory of `voice.json` (`None` in tests: nothing is persisted).
    pub config_dir: Option<std::path::PathBuf>,
    /// Data directory (models).
    pub data_dir: std::path::PathBuf,
}

impl EventEmitter<VoiceControllerEvent> for VoiceController {}

impl Focusable for VoiceController {
    fn focus_handle(&self, _: &crate::ui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl VoiceController {
    /// Starts the runtime described by `config` (nothing when voice is disabled).
    pub fn new(
        cx: &mut Context<Self>,
        config: VoiceConfig,
        dirs: VoiceDirs,
        organism: Option<Entity<OrganismView>>,
    ) -> Self {
        let (tx, rx) = async_channel::unbounded();
        let sink: Arc<dyn Fn(VoiceEvent) + Send + Sync> = Arc::new(move |ev| {
            let _ = tx.send_blocking(ev);
        });
        let service = VoiceService::start(config.clone(), &dirs.data_dir, sink.clone());
        let pump = EventPump::attach(cx, rx, |this: &mut Self, ev, cx| this.on_event(ev, cx));
        let dock = DockModel {
            stt: service.worker_state(Role::Stt),
            tts: service.worker_state(Role::Tts),
            ..DockModel::default()
        };
        Self {
            service: Some(service),
            config,
            dock,
            organism,
            levels: OrganismLevels::default(),
            stats: Stats::default(),
            turn: None,
            epoch: Instant::now(),
            last_tick: Instant::now(),
            ticker: None,
            focus: cx.focus_handle(),
            dirs,
            sink,
            settings_open: false,
            input_devices: Vec::new(),
            output_devices: Vec::new(),
            _pump: pump,
        }
    }

    /// Opens or closes the voice settings panel (device lists are refreshed on open).
    pub fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = !self.settings_open;
        if self.settings_open {
            let host = self.config.host.as_deref();
            let label = t!("voice.settings.default_device").to_string();
            self.input_devices =
                devices::choices(&audio::list_input_devices(host).unwrap_or_default(), &label);
            self.output_devices = devices::choices(
                &audio::list_output_devices(host).unwrap_or_default(),
                &label,
            );
            self.ensure_ticker(cx);
        }
        cx.notify();
    }

    /// Chooses the microphone (`None` = system default) and restarts the runtime on it.
    pub fn select_input(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.config.input_device = id;
        self.reconfigure(cx);
    }

    /// Chooses the speaker (`None` = system default) and restarts the runtime on it.
    pub fn select_output(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.config.output_device = id;
        self.reconfigure(cx);
    }

    /// Switches the voice runtime on or off.
    pub fn set_enabled(&mut self, on: bool, cx: &mut Context<Self>) {
        self.config.enabled = on;
        self.reconfigure(cx);
    }

    /// Persists the configuration and restarts the workers and audio streams on it.
    fn reconfigure(&mut self, cx: &mut Context<Self>) {
        if let Some(dir) = &self.dirs.config_dir
            && let Err(err) = self.config.save(dir)
        {
            tracing::warn!("could not save voice.json: {err}");
        }
        self.service = None;
        let service =
            VoiceService::start(self.config.clone(), &self.dirs.data_dir, self.sink.clone());
        self.dock = DockModel {
            stt: service.worker_state(Role::Stt),
            tts: service.worker_state(Role::Tts),
            ..DockModel::default()
        };
        self.service = Some(service);
        self.sync_body(cx);
    }

    pub(super) fn settings_open_flag(&self) -> bool {
        self.settings_open
    }

    pub(super) fn input_choices(&self) -> Vec<DeviceChoice> {
        self.input_devices.clone()
    }

    pub(super) fn output_choices(&self) -> Vec<DeviceChoice> {
        self.output_devices.clone()
    }

    /// The dock's focus handle (key events for push-to-talk arrive here).
    #[must_use]
    pub fn focus_handle_clone(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// The runtime, for the orchestrator.
    #[must_use]
    pub fn service(&self) -> Option<&VoiceService> {
        self.service.as_ref()
    }

    /// The configuration.
    #[must_use]
    pub fn config(&self) -> &VoiceConfig {
        &self.config
    }

    /// Milliseconds since the controller started (latency clock).
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        super::latency::ms(self.epoch.elapsed())
    }

    /// Records a finished turn in the statistics (logs it too).
    pub fn finish_turn(&mut self, turn: &Turn) {
        self.stats.finish(turn);
    }

    /// Shows the "voice → decision → action" card.
    pub fn set_card(&mut self, card: ActionCard, cx: &mut Context<Self>) {
        self.dock.card = Some(card);
        cx.notify();
    }

    /// Sets the result line of the current card.
    pub fn set_result(&mut self, text: String, tone: ResultTone, cx: &mut Context<Self>) {
        if let Some(card) = self.dock.card.as_mut() {
            card.result = Some((text, tone));
        }
        cx.notify();
    }

    /// Shows a line of text under the orb (what could not be spoken).
    pub fn set_notice_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.dock.notice = Some(Notice::Info(text));
        cx.notify();
    }

    /// The session machine decided the body state: follow it.
    pub fn set_body(&mut self, body: BodyState, cx: &mut Context<Self>) {
        let phase = match body {
            BodyState::Listening => Phase::Listening,
            BodyState::Thinking => Phase::Thinking,
            BodyState::Speaking => Phase::Speaking,
            BodyState::Idle | BodyState::Delegating => Phase::Idle,
        };
        if self.dock.phase != phase {
            self.dock.phase = phase;
            self.sync_body(cx);
        }
    }

    /// Puts the body back to Idle after the answer was handled without speech.
    pub fn settle(&mut self, cx: &mut Context<Self>) {
        if self.dock.phase == Phase::Thinking {
            self.dock.phase = Phase::Idle;
            self.sync_body(cx);
        }
    }

    fn body_state(&self) -> BodyState {
        match self.dock.phase {
            Phase::Idle => BodyState::Idle,
            Phase::Listening => BodyState::Listening,
            Phase::Thinking => BodyState::Thinking,
            Phase::Speaking => BodyState::Speaking,
        }
    }

    fn sync_body(&mut self, cx: &mut Context<Self>) {
        let state = self.body_state();
        if let Some(org) = &self.organism {
            org.update(cx, |o, cx| o.set_state(state, cx));
        }
        if self.dock.phase == Phase::Idle {
            if let Some(org) = &self.organism {
                org.update(cx, |o, _| o.set_levels(None, None));
            }
            self.levels = OrganismLevels::default();
        } else {
            self.ensure_ticker(cx);
        }
        cx.notify();
    }

    fn ensure_ticker(&mut self, cx: &mut Context<Self>) {
        if self.ticker.is_some() {
            return;
        }
        self.last_tick = Instant::now();
        self.ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let keep = this.update(cx, |this, cx| this.tick(cx)).unwrap_or(false);
                if !keep {
                    this.update(cx, |this, _| this.ticker = None).ok();
                    break;
                }
            }
        }));
    }

    /// Publishes mic/TTS levels to the organism; returns false when nothing needs ticking.
    fn tick(&mut self, cx: &mut Context<Self>) -> bool {
        let now = Instant::now();
        let dt = now - self.last_tick;
        self.last_tick = now;
        let Some(service) = &self.service else {
            return false;
        };
        let mic = (self.dock.phase == Phase::Listening)
            .then(|| service.mic_level())
            .flatten();
        let tts = (self.dock.phase == Phase::Speaking)
            .then(|| service.output_rms())
            .flatten();
        let (mic, tts) = self.levels.tick(mic, tts, dt);
        if let Some(org) = &self.organism {
            org.update(cx, |o, _| o.set_levels(mic, tts));
        }
        if self.settings_open {
            cx.notify();
        }
        self.settings_open || matches!(self.dock.phase, Phase::Listening | Phase::Speaking)
    }

    /// Push-to-talk down.
    pub fn press(&mut self, cx: &mut Context<Self>) {
        let Some(service) = &self.service else { return };
        if !self.config.enabled {
            self.dock.refuse(Notice::VoiceOff);
            cx.notify();
            return;
        }
        if !self.dock.can_listen() {
            let notice = match self.dock.mic_problem {
                Some(super::devices::MicProblem::PermissionDenied) => Notice::MicDenied,
                Some(super::devices::MicProblem::NoDevice) => Notice::NoMic,
                _ => Notice::SttUnavailable,
            };
            self.dock.refuse(notice);
            cx.notify();
            return;
        }
        let pressed = service.press();
        if pressed.barge_in {
            cx.emit(VoiceControllerEvent::BargeIn);
        }
        if pressed.started {
            self.dock.begin_listening();
            self.sync_body(cx);
            cx.emit(VoiceControllerEvent::Pressed);
        }
    }

    /// Push-to-talk up.
    pub fn release(&mut self, cx: &mut Context<Self>) {
        let Some(service) = &self.service else { return };
        if !service.is_recording() {
            return;
        }
        let release_ms = self.now_ms();
        let ev = service.release();
        if matches!(ev, PttEvent::Submitted { .. }) {
            let mut turn = Turn::started(release_ms);
            turn.mark(Stage::PcmReady, self.now_ms());
            self.turn = Some(turn);
        }
        self.dock.apply_ptt(&ev);
        self.sync_body(cx);
        cx.emit(VoiceControllerEvent::Released {
            submitted: matches!(ev, PttEvent::Submitted { .. }),
        });
    }

    fn on_event(&mut self, ev: VoiceEvent, cx: &mut Context<Self>) {
        // Synthesized audio goes straight to the speaker.
        if let VoiceEvent::Worker(WorkerEvent::Message {
            role: Role::Tts,
            msg,
        }) = &ev
        {
            match msg {
                FromWorker::Pcm {
                    sample_rate,
                    samples,
                    ..
                } => {
                    if let Some(s) = &self.service {
                        s.play(samples.clone(), *sample_rate);
                    }
                }
                FromWorker::SynthDone { id } => cx.emit(VoiceControllerEvent::SynthFinished {
                    id: *id,
                    failed: false,
                }),
                FromWorker::Error { id: Some(id), .. } => {
                    cx.emit(VoiceControllerEvent::SynthFinished {
                        id: *id,
                        failed: true,
                    });
                }
                _ => {}
            }
        }
        match &ev {
            VoiceEvent::Output(audio::PlaybackEvent::Started) => {
                cx.emit(VoiceControllerEvent::PlaybackStarted);
            }
            VoiceEvent::Output(audio::PlaybackEvent::Drained) => {
                cx.emit(VoiceControllerEvent::PlaybackDrained);
            }
            _ => {}
        }
        let workers_changed = matches!(&ev, VoiceEvent::Worker(WorkerEvent::State { .. }));
        let before = self.dock.phase;
        if let Some(Effect::Transcript { id, text }) = self.dock.apply(&ev) {
            let mut turn = self.turn.take().unwrap_or_default();
            turn.mark(Stage::Transcript, self.now_ms());
            self.turn = Some(turn.clone());
            cx.emit(VoiceControllerEvent::Transcript { id, text, turn });
        }
        if workers_changed {
            cx.emit(VoiceControllerEvent::WorkersChanged);
        }
        if self.dock.phase != before {
            self.sync_body(cx);
        }
        cx.notify();
    }

    /// Takes the in-flight turn (the orchestrator marks the next stages on it).
    pub fn take_turn(&mut self) -> Option<Turn> {
        self.turn.take()
    }

    /// Header text: "Voz disponible · stt" and friends.
    #[must_use]
    pub fn status_label(&self) -> String {
        if !self.config.enabled {
            return t!("voice.status.off").to_string();
        }
        match self.dock.stt {
            WorkerState::Ready => t!("voice.status.ready").to_string(),
            WorkerState::Starting => t!("voice.status.starting").to_string(),
            WorkerState::Restarting => t!("voice.status.restarting").to_string(),
            WorkerState::Failed | WorkerState::Stopped => {
                t!("voice.status.unavailable").to_string()
            }
        }
    }

    fn on_key_down(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        if e.keystroke.key == PTT_KEY && !e.is_held {
            self.press(cx);
        }
    }

    fn on_key_up(&mut self, e: &KeyUpEvent, cx: &mut Context<Self>) {
        if e.keystroke.key == PTT_KEY {
            self.release(cx);
        }
    }
}

fn marfil(c: palette::Rgba) -> Hsla {
    rgba(c.as_u32()).into()
}

fn notice_text(n: &Notice) -> String {
    match n {
        Notice::TooLong => t!("voice.notice.too_long").to_string(),
        Notice::SttUnavailable => t!("voice.notice.stt_unavailable").to_string(),
        Notice::SttError(m) => t!("voice.notice.stt_error", message = m).to_string(),
        Notice::MicDenied => t!("voice.notice.mic_denied").to_string(),
        Notice::NoMic => t!("voice.notice.no_mic").to_string(),
        Notice::VoiceOff => t!("voice.notice.voice_off").to_string(),
        Notice::NothingHeard => t!("voice.notice.nothing_heard").to_string(),
        Notice::Info(text) => text.clone(),
    }
}

/// A microphone glyph drawn with plain divs (no icon font needed).
fn mic_glyph(color: Hsla) -> impl IntoElement {
    v_flex()
        .items_center()
        .gap(px(2.0))
        .child(
            div()
                .w(px(10.0))
                .h(px(16.0))
                .rounded_full()
                .border_1()
                .border_color(color),
        )
        .child(div().w(px(16.0)).h(px(1.0)).bg(color))
}

impl Render for VoiceController {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let listening = self.dock.phase == Phase::Listening;
        let ink = if listening {
            rgb(palette::BOSQUE_DEEP).into()
        } else {
            marfil(palette::MARFIL_ALPHA_62)
        };
        let mic = div()
            .id("voice-mic")
            .size(px(60.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(if listening {
                rgb(palette::ALAMO).into()
            } else {
                marfil(palette::MARFIL_ALPHA_22)
            })
            .bg(if listening {
                rgb(palette::ALAMO).into()
            } else {
                Hsla::transparent_black()
            })
            .child(mic_glyph(ink))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.press(cx)),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.release(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.release(cx)),
            );
        let secondary = marfil(palette::MARFIL_ALPHA_62);
        let caption = if self.dock.caption.is_empty() && listening {
            t!("voice.mic.listening").to_string()
        } else {
            self.dock.caption.clone()
        };
        let text = v_flex()
            .flex_1()
            .min_w_0()
            .gap(px(4.0))
            .child(div().text_size(px(15.0)).child(caption))
            .children(self.dock.notice.as_ref().map(|n| {
                div()
                    .text_size(px(11.0))
                    .text_color(if matches!(n, Notice::NothingHeard | Notice::TooLong) {
                        secondary
                    } else {
                        rgb(palette::BERMELLON).into()
                    })
                    .child(notice_text(n))
            }))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(secondary)
                    .child(self.status_label()),
            );
        let banner = self.permission_banner(cx);
        let settings = self.settings_view(cx);
        let gear = Button::new("voice-gear")
            .ghost()
            .label(t!("voice.settings.title").to_string())
            .on_click(cx.listener(|this, _, _, cx| this.toggle_settings(cx)));
        let row = h_flex()
            .id("voice-dock")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| this.on_key_down(e, cx)))
            .on_key_up(cx.listener(|this, e: &KeyUpEvent, _, cx| this.on_key_up(e, cx)))
            .w_full()
            .gap(px(14.0))
            .px(px(16.0))
            .py(px(10.0))
            .items_center()
            .border_t_1()
            .border_color(marfil(palette::MARFIL_ALPHA_12))
            .child(mic)
            .child(text)
            .children(self.dock.card.as_ref().map(card_view))
            .child(gear);
        v_flex()
            .w_full()
            .children(banner)
            .children(settings)
            .child(row)
    }
}

/// The decision card: question rows with a 1 px probability bar and a monospace result line.
fn card_view(card: &ActionCard) -> impl IntoElement {
    let secondary = marfil(palette::MARFIL_ALPHA_62);
    let rows = card.rows.iter().map(|r| {
        let p = r.p.unwrap_or(0.0).clamp(0.0, 1.0) as f32;
        v_flex()
            .gap(px(2.0))
            .child(
                h_flex()
                    .justify_between()
                    .gap(px(10.0))
                    .text_size(px(11.0))
                    .child(div().text_color(secondary).child(r.label.clone()))
                    .child(div().child(match r.p {
                        Some(p) => format!("{} · {:.0} %", r.value, p * 100.0),
                        None => r.value.clone(),
                    })),
            )
            .child(
                div()
                    .h(px(1.0))
                    .w_full()
                    .bg(marfil(palette::MARFIL_ALPHA_12))
                    .child(
                        div()
                            .h(px(1.0))
                            .w(crate::ui::text_edit::relative(p))
                            .bg(rgb(palette::ALAMO)),
                    ),
            )
    });
    v_flex()
        .id("voice-card")
        .w(px(300.0))
        .flex_shrink_0()
        .gap(px(6.0))
        .p(px(10.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(marfil(palette::MARFIL_ALPHA_22))
        .children(rows)
        .children(card.result.as_ref().map(|(text, tone)| {
            div()
                .mt(px(4.0))
                .px(px(8.0))
                .py(px(4.0))
                .border_1()
                .border_color::<Hsla>(match tone {
                    ResultTone::Ok => rgb(palette::ALAMO).into(),
                    ResultTone::NeedsUser => rgb(palette::ALAMO_OSCURO).into(),
                    ResultTone::Failed => rgb(palette::BERMELLON).into(),
                })
                .font_family(crate::fonts::FONT_MONO)
                .text_size(px(11.0))
                .child(text.clone())
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::AppContext as _;
    use crate::ui::testing::{TestAppContext, gpui_test};

    fn controller(cx: &mut TestAppContext, enabled: bool) -> Entity<VoiceController> {
        cx.update(|cx| {
            crate::ui::init(cx);
            crate::ui::kit_theme::install(cx);
        });
        let dir = tempfile::tempdir().expect("tmp");
        let cfg = VoiceConfig {
            enabled: false,
            ..VoiceConfig::default()
        };
        let path = dir.path().to_path_buf();
        let _ = enabled;
        cx.new(|cx| {
            VoiceController::new(
                cx,
                cfg,
                VoiceDirs {
                    config_dir: None,
                    data_dir: path,
                },
                None,
            )
        })
    }

    #[gpui_test]
    fn pressing_with_voice_off_explains_instead_of_recording(cx: &mut TestAppContext) {
        let c = controller(cx, false);
        c.update(cx, |c, cx| c.press(cx));
        c.read_with(cx, |c, _| {
            assert_eq!(c.dock.notice, Some(Notice::VoiceOff));
            assert_eq!(c.dock.phase, Phase::Idle);
        });
    }

    #[gpui_test]
    fn a_final_transcript_event_reaches_subscribers_with_its_turn(cx: &mut TestAppContext) {
        let c = controller(cx, false);
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = seen.clone();
        let _sub = cx.update(|cx| {
            cx.subscribe(&c, move |_, ev: &VoiceControllerEvent, _| {
                sink.borrow_mut().push(ev.clone());
            })
        });
        c.update(cx, |c, cx| {
            c.turn = Some(Turn::started(0));
            c.dock.apply_ptt(&PttEvent::Submitted {
                id: 5,
                samples: 10,
                too_long: false,
            });
            c.on_event(
                VoiceEvent::Worker(WorkerEvent::Message {
                    role: Role::Stt,
                    msg: FromWorker::Transcript {
                        id: 5,
                        text: "estado".into(),
                        lang: "es".into(),
                        words: vec![],
                        ttft_ms: None,
                    },
                }),
                cx,
            );
        });
        let events = seen.borrow();
        assert!(matches!(
            events.first(),
            Some(VoiceControllerEvent::Transcript { id: 5, text, turn })
                if text == "estado" && turn.stages().len() == 1
        ));
    }
}
