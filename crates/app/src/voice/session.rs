//! The voice session state machine (PDA-T-0113, PDA-T-0114; `docs/06-ui-mapping.md` "Body state
//! machine"). Pure: stages feed [`Input`]s in and execute the returned [`Output`]s, so every
//! transition is tested with doubles.
//!
//! ```text
//! Idle ──ptt──▶ Listening ──release──▶ Transcribing ──transcript──▶ Deciding
//! Deciding ──command──▶ Acting ──done──▶ Idle          (commands never reach the chat)
//! Deciding ──conversation──▶ Thinking ──first delta──▶ Answering ──done──▶ Speaking ──drained──▶ Idle
//! Deciding ──confirm──▶ Confirming ──answered/expired──▶ Acting | Idle
//! Speaking/Thinking/Answering ──ptt──▶ Listening        (barge-in: audio stops, run keeps going)
//! ```

use organism::BodyState;
use speech::{Action, Enqueued, Priority, SpeechQueue, Utterance};

/// Where the session is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Nothing happening (or only a background run).
    Idle,
    /// Recording the user.
    Listening,
    /// Waiting for the STT transcript.
    Transcribing,
    /// Waiting for the decision.
    Deciding,
    /// Waiting for a spoken or clicked confirmation.
    Confirming,
    /// A command is running.
    Acting,
    /// A chat run was started and nothing has been answered yet.
    Thinking,
    /// The answer is streaming to the chat view (it is spoken when the run ends).
    Answering,
    /// Audio is playing.
    Speaking,
}

/// How the decision layer classified a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decided {
    /// Runs now.
    Command,
    /// Needs a yes first.
    Confirm,
    /// Needs a click (never by voice).
    ExplicitOnly,
    /// Talk to the orchestrator.
    Conversation,
    /// Insert into the composer.
    Dictate,
    /// Project switch (a pill update).
    SwitchProject,
}

/// Events from the stages.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// Push-to-talk down.
    PttPressed,
    /// Push-to-talk up; `submitted` is false for a tap.
    PttReleased {
        /// Audio went to the STT worker.
        submitted: bool,
    },
    /// Final transcript.
    Transcript(String),
    /// The STT worker died or refused.
    SttFailed,
    /// Decision made.
    Decided(Decided),
    /// A command finished; `speech` is what to say (status summaries, "listo").
    CommandFinished {
        /// It worked.
        ok: bool,
        /// Spoken result.
        speech: Option<String>,
    },
    /// A confirmation ended (executed -> `CommandFinished` follows).
    ConfirmationClosed {
        /// The command is going to run.
        run: bool,
    },
    /// The first non-empty `content_delta`.
    ChatFirstContent,
    /// The run finished with this speakable answer (`None`: nothing to say).
    ChatFinished(Option<String>),
    /// The run failed.
    ChatFailed,
    /// The stream closed without `done`: the run continues in the background.
    ChatDetached,
    /// A queued utterance started playing.
    SpeechStarted(u64),
    /// An utterance finished or was stopped.
    SpeechFinished(u64),
    /// A notification or conclusion to speak when idle (Claude Stop hook, delegate results).
    Notify {
        /// Text.
        text: String,
        /// Direct beats conclusion beats notification.
        priority: Priority,
        /// Agent that produced it.
        source: Option<String>,
    },
    /// "calla": stop and drop everything.
    Cancel,
}

/// What the stages must do.
#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    /// Set the organism body state.
    SetBody(BodyState),
    /// Stop playback now and cancel the synthesis of the playing utterance.
    StopAudio,
    /// Start speaking (synthesize sentence by sentence).
    Speak(Utterance),
    /// Run the decision on this transcript.
    Decide(String),
    /// Send this to the chat. `steer`: a run is still active, so it goes in as steering.
    Chat {
        /// Text.
        text: String,
        /// Inject into the active run.
        steer: bool,
    },
    /// Cancel the active chat run.
    CancelRun,
    /// Show text only (muted, or nothing could be spoken).
    ShowText(String),
}

/// The state machine.
#[derive(Debug)]
pub struct Session {
    stage: Stage,
    queue: SpeechQueue,
    next_id: u64,
    run_active: bool,
    /// Stage to return to when a barge-in tap ends without audio.
    before_listening: Stage,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// An idle session.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stage: Stage::Idle,
            queue: SpeechQueue::new(),
            next_id: 1,
            run_active: false,
            before_listening: Stage::Idle,
        }
    }

    /// Current stage.
    #[must_use]
    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// A chat run is still going on.
    #[must_use]
    pub fn run_active(&self) -> bool {
        self.run_active
    }

    /// The speech queue (for mute / pulse state).
    pub fn queue_mut(&mut self) -> &mut SpeechQueue {
        &mut self.queue
    }

    /// Body state of the stage.
    #[must_use]
    pub fn body(&self) -> BodyState {
        match self.stage {
            Stage::Idle | Stage::Answering | Stage::Confirming => BodyState::Idle,
            Stage::Listening => BodyState::Listening,
            Stage::Transcribing | Stage::Deciding | Stage::Acting | Stage::Thinking => {
                BodyState::Thinking
            }
            Stage::Speaking => BodyState::Speaking,
        }
    }

    fn utterance(&mut self, text: String, priority: Priority, source: Option<String>) -> Utterance {
        let id = self.next_id;
        self.next_id += 1;
        let mut u = Utterance::new(id, priority, text);
        if let Some(s) = source {
            u = u.from_source(s);
        }
        u
    }

    fn drain(&mut self, actions: Vec<Action>, out: &mut Vec<Output>) {
        for a in actions {
            match a {
                Action::Speak(u) => out.push(Output::Speak(u)),
                Action::Stop { .. } => out.push(Output::StopAudio),
            }
        }
    }

    fn say(
        &mut self,
        text: String,
        priority: Priority,
        source: Option<String>,
        out: &mut Vec<Output>,
    ) {
        let u = self.utterance(text, priority, source);
        match self.queue.enqueue(u) {
            Enqueued::Queued => {
                if let Some(a) = self.queue.poll() {
                    self.drain(vec![a], out);
                }
            }
            Enqueued::Muted(u) => out.push(Output::ShowText(u.text)),
        }
    }

    /// Whether anything is playing or waiting to play.
    fn speech_busy(&self) -> bool {
        !self.queue.is_idle()
    }

    fn settle(&mut self) {
        self.stage = if self.speech_busy() {
            Stage::Speaking
        } else {
            Stage::Idle
        };
    }

    /// Applies one input.
    pub fn step(&mut self, input: Input) -> Vec<Output> {
        let before = self.body();
        let mut out = Vec::new();
        match input {
            Input::PttPressed => {
                if self.stage != Stage::Listening {
                    self.before_listening = self.stage;
                    let actions = self.queue.set_listening(true);
                    self.drain(actions, &mut out);
                    self.stage = Stage::Listening;
                }
            }
            Input::PttReleased { submitted } if self.stage == Stage::Listening => {
                let actions = self.queue.set_listening(false);
                if submitted {
                    // Whatever was queued stays queued until the answer is spoken.
                    let _ = actions;
                    self.stage = Stage::Transcribing;
                } else {
                    self.drain(actions, &mut out);
                    self.stage = match self.before_listening {
                        Stage::Thinking => Stage::Thinking,
                        Stage::Answering => Stage::Answering,
                        _ => Stage::Idle,
                    };
                    if self.stage == Stage::Idle {
                        self.settle();
                    }
                }
            }
            Input::Transcript(text) if self.stage == Stage::Transcribing => {
                self.stage = Stage::Deciding;
                out.push(Output::Decide(text));
            }
            Input::SttFailed if matches!(self.stage, Stage::Transcribing | Stage::Listening) => {
                let actions = self.queue.set_listening(false);
                self.drain(actions, &mut out);
                self.settle();
            }
            Input::Decided(kind) if self.stage == Stage::Deciding => match kind {
                Decided::Command => self.stage = Stage::Acting,
                Decided::Confirm => self.stage = Stage::Confirming,
                Decided::ExplicitOnly | Decided::Dictate | Decided::SwitchProject => self.settle(),
                Decided::Conversation => {
                    // Chat text is sent by the caller, which still has the transcript.
                    self.stage = Stage::Thinking;
                }
            },
            Input::CommandFinished { speech, .. } if self.stage == Stage::Acting => {
                self.stage = Stage::Idle;
                if let Some(text) = speech {
                    self.say(text, Priority::Direct, None, &mut out);
                }
                self.settle();
            }
            Input::ConfirmationClosed { run } if self.stage == Stage::Confirming => {
                self.stage = if run { Stage::Acting } else { Stage::Idle };
                if !run {
                    self.settle();
                }
            }
            Input::ChatFirstContent if self.stage == Stage::Thinking => {
                self.run_active = true;
                self.stage = Stage::Answering;
            }
            Input::ChatFinished(answer) => {
                self.run_active = false;
                if matches!(self.stage, Stage::Thinking | Stage::Answering) {
                    self.stage = Stage::Idle;
                }
                if let Some(text) = answer {
                    self.say(text, Priority::Direct, None, &mut out);
                }
                if self.stage == Stage::Idle {
                    self.settle();
                }
            }
            Input::ChatFailed | Input::ChatDetached => {
                self.run_active = matches!(input, Input::ChatDetached);
                if matches!(self.stage, Stage::Thinking | Stage::Answering) {
                    self.stage = Stage::Idle;
                    self.settle();
                }
            }
            Input::SpeechStarted(_) => {
                if matches!(self.stage, Stage::Idle | Stage::Answering) {
                    self.stage = Stage::Speaking;
                }
            }
            Input::SpeechFinished(id) => {
                let actions = self.queue.finished(id);
                self.drain(actions, &mut out);
                if self.stage == Stage::Speaking && !self.speech_busy() {
                    self.stage = Stage::Idle;
                }
            }
            Input::Notify {
                text,
                priority,
                source,
            } => {
                self.say(text, priority, source, &mut out);
                if self.stage == Stage::Idle && self.speech_busy() {
                    self.stage = Stage::Speaking;
                }
            }
            Input::Cancel => {
                let actions = self.queue.skip_all();
                self.drain(actions, &mut out);
                if matches!(self.stage, Stage::Thinking | Stage::Answering) {
                    out.push(Output::CancelRun);
                    self.run_active = false;
                }
                if !matches!(
                    self.stage,
                    Stage::Listening | Stage::Transcribing | Stage::Deciding
                ) {
                    self.stage = Stage::Idle;
                }
            }
            _ => {}
        }
        let after = self.body();
        if after != before {
            out.push(Output::SetBody(after));
        }
        out
    }

    /// How a conversation transcript must be sent: as steering when a run is active.
    #[must_use]
    pub fn chat_output(&self, text: String) -> Output {
        Output::Chat {
            text,
            steer: self.run_active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(out: &[Output]) -> Vec<BodyState> {
        out.iter()
            .filter_map(|o| {
                if let Output::SetBody(b) = o {
                    Some(*b)
                } else {
                    None
                }
            })
            .collect()
    }

    fn has_speak(out: &[Output]) -> Option<Utterance> {
        out.iter().find_map(|o| {
            if let Output::Speak(u) = o {
                Some(u.clone())
            } else {
                None
            }
        })
    }

    fn to_deciding(s: &mut Session) {
        assert_eq!(body(&s.step(Input::PttPressed)), [BodyState::Listening]);
        assert_eq!(
            body(&s.step(Input::PttReleased { submitted: true })),
            [BodyState::Thinking]
        );
        let out = s.step(Input::Transcript("hola".into()));
        assert_eq!(out, [Output::Decide("hola".into())]);
        assert_eq!(s.stage(), Stage::Deciding);
    }

    #[test]
    fn a_command_goes_idle_without_touching_the_chat() {
        let mut s = Session::new();
        to_deciding(&mut s);
        s.step(Input::Decided(Decided::Command));
        assert_eq!(s.stage(), Stage::Acting);
        let out = s.step(Input::CommandFinished {
            ok: true,
            speech: None,
        });
        assert_eq!(body(&out), [BodyState::Idle]);
        assert!(!out.iter().any(|o| matches!(o, Output::Chat { .. })));
        assert_eq!(s.stage(), Stage::Idle);
    }

    #[test]
    fn a_command_with_a_spoken_result_ends_speaking_then_idle() {
        let mut s = Session::new();
        to_deciding(&mut s);
        s.step(Input::Decided(Decided::Command));
        let out = s.step(Input::CommandFinished {
            ok: true,
            speech: Some("va al 40 %".into()),
        });
        let u = has_speak(&out).expect("spoken");
        assert_eq!(s.stage(), Stage::Speaking);
        let out = s.step(Input::SpeechFinished(u.id));
        assert_eq!(body(&out), [BodyState::Idle]);
    }

    #[test]
    fn a_conversation_thinks_then_answers_then_speaks_then_idles() {
        let mut s = Session::new();
        to_deciding(&mut s);
        s.step(Input::Decided(Decided::Conversation));
        assert_eq!(s.stage(), Stage::Thinking);
        assert_eq!(
            s.chat_output("hola".into()),
            Output::Chat {
                text: "hola".into(),
                steer: false
            }
        );
        let out = s.step(Input::ChatFirstContent);
        assert_eq!(
            body(&out),
            [BodyState::Idle],
            "Thinking ends at the first delta"
        );
        assert!(s.run_active());
        let out = s.step(Input::ChatFinished(Some("Hola, ¿qué tal?".into())));
        let u = has_speak(&out).expect("spoken");
        assert_eq!(body(&out), [BodyState::Speaking]);
        assert!(!s.run_active());
        s.step(Input::SpeechStarted(u.id));
        let out = s.step(Input::SpeechFinished(u.id));
        assert_eq!(body(&out), [BodyState::Idle]);
        assert_eq!(s.stage(), Stage::Idle);
    }

    #[test]
    fn silent_tool_work_goes_thinking_to_idle_without_speech() {
        let mut s = Session::new();
        to_deciding(&mut s);
        s.step(Input::Decided(Decided::Conversation));
        let out = s.step(Input::ChatFinished(None));
        assert!(has_speak(&out).is_none());
        assert_eq!(body(&out), [BodyState::Idle]);
    }

    #[test]
    fn a_stream_closed_without_done_detaches_to_the_background() {
        let mut s = Session::new();
        to_deciding(&mut s);
        s.step(Input::Decided(Decided::Conversation));
        s.step(Input::ChatDetached);
        assert_eq!(s.stage(), Stage::Idle);
        assert!(s.run_active(), "the run continues");
        assert_eq!(
            s.chat_output("y también esto".into()),
            Output::Chat {
                text: "y también esto".into(),
                steer: true
            }
        );
    }

    #[test]
    fn a_tap_returns_to_idle() {
        let mut s = Session::new();
        s.step(Input::PttPressed);
        let out = s.step(Input::PttReleased { submitted: false });
        assert_eq!(body(&out), [BodyState::Idle]);
    }

    #[test]
    fn barge_in_stops_the_audio_and_listens_even_if_the_run_continues() {
        let mut s = Session::new();
        to_deciding(&mut s);
        s.step(Input::Decided(Decided::Conversation));
        s.step(Input::ChatFirstContent);
        let out = s.step(Input::ChatFinished(Some("respuesta larga".into())));
        let u = has_speak(&out).expect("spoken");
        s.step(Input::SpeechStarted(u.id));
        assert_eq!(s.stage(), Stage::Speaking);
        let out = s.step(Input::PttPressed);
        assert!(out.contains(&Output::StopAudio), "{out:?}");
        assert_eq!(body(&out), [BodyState::Listening]);
        // While listening, nothing else is spoken.
        let out = s.step(Input::Notify {
            text: "aviso".into(),
            priority: Priority::Notification,
            source: None,
        });
        assert!(has_speak(&out).is_none());
    }

    #[test]
    fn a_steering_prompt_goes_into_the_active_run() {
        let mut s = Session::new();
        to_deciding(&mut s);
        s.step(Input::Decided(Decided::Conversation));
        s.step(Input::ChatFirstContent);
        s.step(Input::PttPressed);
        s.step(Input::PttReleased { submitted: true });
        s.step(Input::Transcript("mejor hazlo en rust".into()));
        s.step(Input::Decided(Decided::Conversation));
        assert_eq!(
            s.chat_output("mejor hazlo en rust".into()),
            Output::Chat {
                text: "mejor hazlo en rust".into(),
                steer: true
            }
        );
    }

    #[test]
    fn notifications_wait_while_listening_and_speak_afterwards() {
        let mut s = Session::new();
        s.step(Input::PttPressed);
        let out = s.step(Input::Notify {
            text: "migracion-auth ha terminado".into(),
            priority: Priority::Conclusion,
            source: Some("migracion-auth".into()),
        });
        assert!(has_speak(&out).is_none());
        let out = s.step(Input::PttReleased { submitted: false });
        assert!(
            has_speak(&out).is_some(),
            "spoken once the user stops: {out:?}"
        );
        assert_eq!(s.stage(), Stage::Speaking);
    }

    #[test]
    fn stt_failure_returns_to_idle() {
        let mut s = Session::new();
        s.step(Input::PttPressed);
        s.step(Input::PttReleased { submitted: true });
        let out = s.step(Input::SttFailed);
        assert_eq!(body(&out), [BodyState::Idle]);
        assert_eq!(s.stage(), Stage::Idle);
    }

    #[test]
    fn confirmation_flow_runs_or_dismisses() {
        let mut s = Session::new();
        to_deciding(&mut s);
        s.step(Input::Decided(Decided::Confirm));
        assert_eq!(s.stage(), Stage::Confirming);
        s.step(Input::ConfirmationClosed { run: true });
        assert_eq!(s.stage(), Stage::Acting);
        s.step(Input::CommandFinished {
            ok: true,
            speech: None,
        });

        let mut s = Session::new();
        to_deciding(&mut s);
        s.step(Input::Decided(Decided::Confirm));
        s.step(Input::ConfirmationClosed { run: false });
        assert_eq!(s.stage(), Stage::Idle);
    }

    #[test]
    fn explicit_only_and_dictation_never_reach_the_chat_or_a_command() {
        for kind in [
            Decided::ExplicitOnly,
            Decided::Dictate,
            Decided::SwitchProject,
        ] {
            let mut s = Session::new();
            to_deciding(&mut s);
            let out = s.step(Input::Decided(kind));
            assert_eq!(s.stage(), Stage::Idle);
            assert!(!out.iter().any(|o| matches!(o, Output::Chat { .. })));
        }
    }

    #[test]
    fn cancel_stops_everything() {
        let mut s = Session::new();
        to_deciding(&mut s);
        s.step(Input::Decided(Decided::Conversation));
        s.step(Input::ChatFirstContent);
        let out = s.step(Input::Cancel);
        assert!(out.contains(&Output::CancelRun));
        assert_eq!(s.stage(), Stage::Idle);
        assert!(!s.run_active());
    }

    #[test]
    fn muted_speech_is_shown_as_text() {
        let mut s = Session::new();
        let _ = s.queue_mut().set_muted(true);
        let out = s.step(Input::Notify {
            text: "hecho".into(),
            priority: Priority::Direct,
            source: None,
        });
        assert_eq!(out, [Output::ShowText("hecho".into())]);
    }
}
