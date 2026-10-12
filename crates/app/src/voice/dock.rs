//! Pure model of the voice dock (PDA-T-0094/0095): the phase of the body, the live caption,
//! worker availability, notices and the "voice → decision → action" card.

use voice_proto::{FromWorker, Role};

use super::devices::MicProblem;
use super::service::{MicEvent, PttEvent, VoiceEvent};
use super::supervisor::{WorkerEvent, WorkerState};

/// What the body (organism) is doing from the voice point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Nothing happening.
    Idle,
    /// Recording the user.
    Listening,
    /// Waiting for the transcript / decision / answer.
    Thinking,
    /// The assistant is speaking.
    Speaking,
}

/// A short message under the orb. Localised by the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// The 120 s limit ended the recording.
    TooLong,
    /// The STT worker is down; nothing was transcribed.
    SttUnavailable,
    /// The STT worker reported an error.
    SttError(String),
    /// The OS refused microphone access.
    MicDenied,
    /// There is no microphone.
    NoMic,
    /// Voice is switched off in the settings.
    VoiceOff,
    /// Nothing intelligible was heard.
    NothingHeard,
}

/// One probability row of the card ("question → answer, p").
#[derive(Debug, Clone, PartialEq)]
pub struct CardRow {
    /// What was asked (`intent`, `agent`, `action`, ...).
    pub label: String,
    /// The chosen answer.
    pub value: String,
    /// Probability of the answer, when valid.
    pub p: Option<f64>,
}

/// Tone of the result line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultTone {
    /// Done / informational.
    Ok,
    /// Needs the user (confirmation, explicit approval).
    NeedsUser,
    /// Could not do it.
    Failed,
}

/// The card showing how a spoken sentence was understood.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActionCard {
    /// The transcript the decision was made on.
    pub transcript: String,
    /// Probability rows.
    pub rows: Vec<CardRow>,
    /// Monospace result line (e.g. `cancel migracion-auth → done`).
    pub result: Option<(String, ResultTone)>,
}

/// The dock state.
#[derive(Debug, Clone, PartialEq)]
pub struct DockModel {
    /// Body phase.
    pub phase: Phase,
    /// Live caption (partial) or the final transcript.
    pub caption: String,
    /// Request id the caption belongs to.
    pub caption_id: Option<u64>,
    /// Message under the orb.
    pub notice: Option<Notice>,
    /// STT availability.
    pub stt: WorkerState,
    /// TTS availability.
    pub tts: WorkerState,
    /// Microphone problem, if any.
    pub mic_problem: Option<MicProblem>,
    /// The microphone stream is running.
    pub mic_open: bool,
    /// The last decision card.
    pub card: Option<ActionCard>,
}

impl Default for DockModel {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            caption: String::new(),
            caption_id: None,
            notice: None,
            stt: WorkerState::Stopped,
            tts: WorkerState::Stopped,
            mic_problem: None,
            mic_open: false,
            card: None,
        }
    }
}

/// Side effect of an event the controller must act on.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A final transcript is ready for the decision layer.
    Transcript {
        /// STT request id.
        id: u64,
        /// Text.
        text: String,
    },
}

impl DockModel {
    /// Whether the user can talk right now.
    #[must_use]
    pub fn can_listen(&self) -> bool {
        self.stt.is_ready() && self.mic_problem.is_none()
    }

    /// Starts a recording.
    pub fn begin_listening(&mut self) {
        self.phase = Phase::Listening;
        self.caption.clear();
        self.caption_id = None;
        self.notice = None;
    }

    /// Voice is off or the worker is not available.
    pub fn refuse(&mut self, notice: Notice) {
        self.phase = Phase::Idle;
        self.notice = Some(notice);
    }

    /// Applies a push-to-talk outcome.
    pub fn apply_ptt(&mut self, ev: &PttEvent) {
        match ev {
            PttEvent::Tap => {
                self.phase = Phase::Idle;
                self.caption.clear();
            }
            PttEvent::Submitted { id, too_long, .. } => {
                self.phase = Phase::Thinking;
                self.caption_id = Some(*id);
                if *too_long {
                    self.notice = Some(Notice::TooLong);
                }
            }
            PttEvent::SttUnavailable => self.refuse(Notice::SttUnavailable),
        }
    }

    /// Applies a runtime event; returns what the controller must do next.
    pub fn apply(&mut self, ev: &VoiceEvent) -> Option<Effect> {
        match ev {
            VoiceEvent::Worker(WorkerEvent::State { role, state, .. }) => {
                match role {
                    Role::Stt => {
                        self.stt = *state;
                        if !state.is_ready() && self.phase != Phase::Speaking {
                            // A dead worker cannot answer the pending request.
                            if self.phase == Phase::Thinking {
                                self.phase = Phase::Idle;
                                self.notice = Some(Notice::SttUnavailable);
                            }
                        }
                    }
                    Role::Tts => self.tts = *state,
                }
                None
            }
            VoiceEvent::Worker(WorkerEvent::Message { msg, .. }) => self.apply_message(msg),
            VoiceEvent::Mic(m) => {
                match m {
                    MicEvent::Started { .. } | MicEvent::Restarted => {
                        self.mic_open = true;
                        self.mic_problem = None;
                        if matches!(self.notice, Some(Notice::MicDenied | Notice::NoMic)) {
                            self.notice = None;
                        }
                    }
                    MicEvent::Problem { kind, .. } => {
                        self.mic_open = false;
                        self.mic_problem = Some(*kind);
                        self.notice = match kind {
                            MicProblem::PermissionDenied => Some(Notice::MicDenied),
                            MicProblem::NoDevice => Some(Notice::NoMic),
                            MicProblem::Other => self.notice.take(),
                        };
                    }
                }
                None
            }
            VoiceEvent::Ptt(p) => {
                self.apply_ptt(p);
                None
            }
            VoiceEvent::Output(audio::PlaybackEvent::Started) => {
                if self.phase != Phase::Listening {
                    self.phase = Phase::Speaking;
                }
                None
            }
            VoiceEvent::Output(audio::PlaybackEvent::Drained) => {
                if self.phase == Phase::Speaking {
                    self.phase = Phase::Idle;
                }
                None
            }
            VoiceEvent::Output(_) => None,
        }
    }

    fn apply_message(&mut self, msg: &FromWorker) -> Option<Effect> {
        match msg {
            FromWorker::Partial { id, text }
                if Some(*id) == self.caption_id || self.phase == Phase::Listening =>
            {
                self.caption.clone_from(text);
                None
            }
            FromWorker::Transcript { id, text, .. } if Some(*id) == self.caption_id => {
                let text = text.trim().to_owned();
                if text.is_empty() {
                    self.phase = Phase::Idle;
                    self.notice = Some(Notice::NothingHeard);
                    self.caption.clear();
                    return None;
                }
                self.caption.clone_from(&text);
                self.phase = Phase::Thinking;
                Some(Effect::Transcript { id: *id, text })
            }
            FromWorker::Error { id, message, .. } if id.is_none() || *id == self.caption_id => {
                if self.phase == Phase::Thinking || self.phase == Phase::Listening {
                    self.phase = Phase::Idle;
                }
                self.notice = Some(Notice::SttError(message.clone()));
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(m: FromWorker) -> VoiceEvent {
        VoiceEvent::Worker(WorkerEvent::Message {
            role: Role::Stt,
            msg: m,
        })
    }

    fn transcript(id: u64, text: &str) -> FromWorker {
        FromWorker::Transcript {
            id,
            text: text.into(),
            lang: "es".into(),
            words: vec![],
            ttft_ms: None,
        }
    }

    #[test]
    fn listening_then_submit_then_final_transcript() {
        let mut d = DockModel::default();
        d.begin_listening();
        assert_eq!(d.phase, Phase::Listening);
        d.apply(&msg(FromWorker::Partial {
            id: 4,
            text: "cancela la".into(),
        }));
        assert_eq!(d.caption, "cancela la");
        d.apply(&VoiceEvent::Ptt(PttEvent::Submitted {
            id: 4,
            samples: 16_000,
            too_long: false,
        }));
        assert_eq!(d.phase, Phase::Thinking);
        let fx = d.apply(&msg(transcript(4, " cancela la migración ")));
        assert_eq!(
            fx,
            Some(Effect::Transcript {
                id: 4,
                text: "cancela la migración".into()
            })
        );
        assert_eq!(d.caption, "cancela la migración");
    }

    #[test]
    fn a_tap_returns_to_idle_without_notice() {
        let mut d = DockModel::default();
        d.begin_listening();
        d.apply_ptt(&PttEvent::Tap);
        assert_eq!(d.phase, Phase::Idle);
        assert!(d.notice.is_none());
    }

    #[test]
    fn stale_transcripts_are_ignored_and_empty_ones_say_nothing_heard() {
        let mut d = DockModel::default();
        d.apply_ptt(&PttEvent::Submitted {
            id: 2,
            samples: 1,
            too_long: false,
        });
        assert_eq!(d.apply(&msg(transcript(1, "viejo"))), None);
        assert_eq!(d.phase, Phase::Thinking);
        assert_eq!(d.apply(&msg(transcript(2, "  "))), None);
        assert_eq!(d.phase, Phase::Idle);
        assert_eq!(d.notice, Some(Notice::NothingHeard));
    }

    #[test]
    fn worker_death_while_thinking_is_reported() {
        let mut d = DockModel::default();
        d.apply_ptt(&PttEvent::Submitted {
            id: 2,
            samples: 1,
            too_long: true,
        });
        assert_eq!(d.notice, Some(Notice::TooLong));
        d.apply(&VoiceEvent::Worker(WorkerEvent::State {
            role: Role::Stt,
            state: WorkerState::Restarting,
            detail: Some("exited".into()),
        }));
        assert_eq!(d.phase, Phase::Idle);
        assert_eq!(d.notice, Some(Notice::SttUnavailable));
        assert!(!d.can_listen());
        d.apply(&VoiceEvent::Worker(WorkerEvent::State {
            role: Role::Stt,
            state: WorkerState::Ready,
            detail: None,
        }));
        assert!(d.can_listen());
    }

    #[test]
    fn mic_denial_blocks_listening_and_recovers() {
        let mut d = DockModel {
            stt: WorkerState::Ready,
            ..DockModel::default()
        };
        d.apply(&VoiceEvent::Mic(MicEvent::Problem {
            kind: MicProblem::PermissionDenied,
            message: "denied".into(),
        }));
        assert_eq!(d.notice, Some(Notice::MicDenied));
        assert!(!d.can_listen());
        d.apply(&VoiceEvent::Mic(MicEvent::Started { device: "x".into() }));
        assert!(d.can_listen());
        assert!(d.notice.is_none());
    }

    #[test]
    fn playback_drives_the_speaking_phase_but_never_interrupts_listening() {
        let mut d = DockModel::default();
        d.apply(&VoiceEvent::Output(audio::PlaybackEvent::Started));
        assert_eq!(d.phase, Phase::Speaking);
        d.apply(&VoiceEvent::Output(audio::PlaybackEvent::Drained));
        assert_eq!(d.phase, Phase::Idle);
        d.begin_listening();
        d.apply(&VoiceEvent::Output(audio::PlaybackEvent::Started));
        assert_eq!(d.phase, Phase::Listening);
    }

    #[test]
    fn stt_errors_surface() {
        let mut d = DockModel::default();
        d.apply_ptt(&PttEvent::Submitted {
            id: 9,
            samples: 1,
            too_long: false,
        });
        d.apply(&msg(FromWorker::Error {
            id: Some(9),
            code: voice_proto::ErrorCode::Engine,
            message: "boom".into(),
        }));
        assert_eq!(d.phase, Phase::Idle);
        assert_eq!(d.notice, Some(Notice::SttError("boom".into())));
    }
}
