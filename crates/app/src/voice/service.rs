//! The voice runtime: microphone capture, speaker playback and the two supervised workers
//! behind one thread-safe handle (PDA-T-0090/0094).
//!
//! Everything here runs on plain threads; results travel to the UI as [`VoiceEvent`]s through
//! a sink (the app wires it to an `async_channel` drained by the GPUI `EventPump`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use audio::bargein::{BargeAction, BargeIn};
use audio::{Capture, CaptureConfig, CaptureEvent, Playback, PlaybackConfig, PlaybackEvent};
use voice_proto::{Role, ToWorker};

use super::config::VoiceConfig;
use super::devices::{self, MicProblem};
use super::ptt::{Finished, Pressed, Ptt};
use super::supervisor::{
    Backoff, EventSink, LoadSpec, ProcessLauncher, SendError, Supervisor, WorkerEvent, WorkerState,
    locate_worker,
};

/// Hands-free audio is sent in blocks of 100 ms.
pub const HF_BLOCK_SAMPLES: usize = 1_600;
/// Samples buffered before a streaming chunk is sent (750 ms at 16 kHz).
pub const STREAM_CHUNK_SAMPLES: usize = 12_000;

/// Everything the voice runtime reports.
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceEvent {
    /// A worker changed state or sent a message.
    Worker(WorkerEvent),
    /// Microphone news.
    Mic(MicEvent),
    /// Speaker news.
    Output(PlaybackEvent),
    /// The push-to-talk recording ended.
    Ptt(PttEvent),
    /// Hands-free barge-in: the user kept talking over the assistant, playback was stopped.
    BargeIn,
}

/// Microphone lifecycle.
#[derive(Debug, Clone, PartialEq)]
pub enum MicEvent {
    /// The stream is running.
    Started {
        /// Device name.
        device: String,
    },
    /// The stream was rebuilt.
    Restarted,
    /// Opening failed (retried every second).
    Problem {
        /// Classified cause.
        kind: MicProblem,
        /// Backend message (never contains secrets).
        message: String,
    },
}

/// Outcome of a push-to-talk recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PttEvent {
    /// A press shorter than 250 ms: back to Idle, nothing transcribed.
    Tap,
    /// The audio was handed to the STT worker as request `id`.
    Submitted {
        /// Request id of the final transcript.
        id: u64,
        /// Samples recorded (16 kHz).
        samples: usize,
        /// The 120 s limit ended the recording.
        too_long: bool,
    },
    /// The STT worker is not available: nothing could be transcribed.
    SttUnavailable,
}

struct Inner {
    hands_free: bool,
    hf_buf: Vec<f32>,
    vad: super::vad::EnergyVad,
    barge: BargeIn,
    ptt: Ptt,
    sent: usize,
    request: u64,
    keywords: Vec<String>,
}

/// The voice runtime handle.
#[allow(missing_debug_implementations)]
pub struct VoiceService {
    inner: Arc<Mutex<Inner>>,
    capture: Option<Capture>,
    playback: Option<Arc<Playback>>,
    stt: Option<Arc<Supervisor>>,
    tts: Option<Supervisor>,
    sink: Arc<dyn Fn(VoiceEvent) + Send + Sync>,
    next_id: AtomicU64,
    epoch: Instant,
    cfg: VoiceConfig,
    tap: Option<audio::playback::ReferenceTap>,
}

impl VoiceService {
    /// Starts whatever the configuration enables. With `enabled == false` nothing is started.
    pub fn start(
        cfg: VoiceConfig,
        data_dir: &std::path::Path,
        sink: Arc<dyn Fn(VoiceEvent) + Send + Sync>,
    ) -> Self {
        let barge_ok = cfg.barge_in && (cfg!(feature = "echo-cancel") || cfg.headset);
        let inner = Arc::new(Mutex::new(Inner {
            hands_free: false,
            hf_buf: Vec::new(),
            vad: super::vad::EnergyVad::new(),
            barge: BargeIn::new(barge_ok),
            ptt: Ptt::new(),
            sent: 0,
            request: 0,
            keywords: Vec::new(),
        }));
        let mut me = Self {
            inner,
            capture: None,
            playback: None,
            stt: None,
            tts: None,
            sink,
            next_id: AtomicU64::new(1),
            epoch: Instant::now(),
            cfg,
            tap: cfg!(feature = "echo-cancel").then(audio::playback::ReferenceTap::new),
        };
        if me.cfg.enabled {
            me.start_stt(data_dir);
            me.start_tts();
            me.start_audio();
        }
        me
    }

    fn worker_sink(&self) -> EventSink {
        let sink = self.sink.clone();
        Arc::new(move |ev| sink(VoiceEvent::Worker(ev)))
    }

    fn start_stt(&mut self, data_dir: &std::path::Path) {
        let Some(model) = super::resolve::stt_model(&self.cfg, data_dir) else {
            tracing::warn!("voice is enabled but no STT model was found");
            (self.sink)(VoiceEvent::Worker(WorkerEvent::State {
                role: Role::Stt,
                state: WorkerState::Failed,
                detail: Some("no STT model found".to_owned()),
            }));
            return;
        };
        let launcher = ProcessLauncher {
            program: locate_worker("pando-stt"),
            args: Vec::new(),
        };
        self.stt = Some(Arc::new(Supervisor::start(
            LoadSpec::Stt {
                model_path: model.display().to_string(),
            },
            launcher,
            Backoff::default(),
            self.worker_sink(),
        )));
    }

    fn start_tts(&mut self) {
        if !self.cfg.speak {
            return;
        }
        let Some(dir) = super::resolve::tts_model_dir(&self.cfg) else {
            return;
        };
        let launcher = ProcessLauncher {
            program: locate_worker("pando-tts"),
            args: Vec::new(),
        };
        self.tts = Some(Supervisor::start(
            LoadSpec::Tts {
                model_dir: dir.display().to_string(),
                voice: self.cfg.tts_voice.clone(),
            },
            launcher,
            Backoff::default(),
            self.worker_sink(),
        ));
        let sink = self.sink.clone();
        self.playback = Some(Arc::new(Playback::start(
            PlaybackConfig {
                device: self.cfg.output_device.clone(),
                host: self.cfg.host.clone(),
                reference: self.tap.clone(),
                ..PlaybackConfig::default()
            },
            move |ev| sink(VoiceEvent::Output(ev)),
        )));
    }

    fn start_audio(&mut self) {
        let inner = self.inner.clone();
        let stt = self.stt.clone();
        let (epoch, lang) = (self.epoch, self.cfg.language.clone());
        let sink = self.sink.clone();
        let playback = self.playback.clone();
        let on_frame = move |frame: &[f32]| {
            let now = elapsed_ms(epoch);
            let Ok(mut g) = inner.lock() else { return };
            // Hands-free: stream the cleaned microphone to the STT worker (VAD + wake there)
            // and watch for the user talking over the assistant.
            let playing = g.hands_free && playback.as_ref().is_some_and(|p| p.is_playing());
            let speech = g.vad.frame(frame);
            match g.barge.frame(playing, speech) {
                BargeAction::Duck => {
                    tracing::debug!("hands-free barge-in: duck");
                    if let Some(p) = &playback {
                        p.set_duck(true);
                    }
                }
                BargeAction::Restore => {
                    if let Some(p) = &playback {
                        p.set_duck(false);
                    }
                }
                BargeAction::Stop => {
                    tracing::debug!("hands-free barge-in: stop");
                    if let Some(p) = &playback {
                        p.set_duck(false);
                        p.stop();
                    }
                    sink(VoiceEvent::BargeIn);
                }
                BargeAction::None => {}
            }
            if g.hands_free && !g.ptt.is_recording() {
                g.hf_buf.extend_from_slice(frame);
                if g.hf_buf.len() >= HF_BLOCK_SAMPLES
                    && let Some(stt) = &stt
                {
                    let pcm = std::mem::take(&mut g.hf_buf);
                    let _ = stt.send(ToWorker::Audio { pcm });
                }
            }
            let finished = g.ptt.frame(frame, now);
            if g.ptt.is_recording()
                && g.ptt.len().saturating_sub(g.sent) >= STREAM_CHUNK_SAMPLES
                && let Some(stt) = &stt
            {
                let chunk = g.ptt.since(g.sent).to_vec();
                let (id, kw) = (g.request, g.keywords.clone());
                g.sent += chunk.len();
                let _ = stt.send(ToWorker::StreamChunk {
                    id,
                    pcm: chunk,
                    lang: Some(lang.clone()),
                    keywords: kw,
                });
            }
            if let Some(Finished::TooLong { pcm }) = finished {
                let (id, sent) = (g.request, g.sent);
                g.sent = 0;
                drop(g);
                if let Some(ev) = submit(stt.as_deref(), id, &pcm, sent, &lang, Vec::new(), true) {
                    sink(VoiceEvent::Ptt(ev));
                }
            }
        };
        let sink = self.sink.clone();
        let on_event = move |ev: CaptureEvent| {
            let mic = match ev {
                CaptureEvent::Started { device, .. } => MicEvent::Started { device },
                CaptureEvent::Restarted { .. } => MicEvent::Restarted,
                CaptureEvent::Error(message) => MicEvent::Problem {
                    kind: devices::classify(&message),
                    message,
                },
            };
            sink(VoiceEvent::Mic(mic));
        };
        #[cfg(feature = "echo-cancel")]
        let echo = self.tap.clone().map(audio::EchoCancel::sonora);
        self.capture = Some(Capture::start(
            CaptureConfig {
                device: self.cfg.input_device.clone(),
                host: self.cfg.host.clone(),
                #[cfg(feature = "echo-cancel")]
                echo,
                ..CaptureConfig::default()
            },
            on_frame,
            on_event,
        ));
    }

    fn now_ms(&self) -> u64 {
        elapsed_ms(self.epoch)
    }

    /// Switches hands-free listening on or off (also re-sent when the STT worker restarts).
    pub fn set_hands_free(&mut self, on: bool) {
        self.cfg.hands_free = on;
        self.apply_hands_free();
    }

    /// Sends the hands-free configuration to the STT worker when it is ready.
    pub fn apply_hands_free(&self) {
        let on = self.cfg.hands_free && self.cfg.enabled;
        let keywords = self
            .inner
            .lock()
            .map(|g| g.keywords.clone())
            .unwrap_or_default();
        if let Ok(mut g) = self.inner.lock() {
            g.hands_free = on;
            g.hf_buf.clear();
        }
        if let Some(stt) = &self.stt {
            let _ = stt.send(ToWorker::HandsFree {
                enabled: on,
                lang: Some(self.cfg.language.clone()),
                keywords,
                sensitivity: self.cfg.wake_sensitivity.clamp(0.0, 1.0),
                vad_model: None,
            });
        }
    }

    /// Changes the wake sensitivity and re-sends the hands-free configuration.
    pub fn set_wake_sensitivity(&mut self, value: f32) {
        self.cfg.wake_sensitivity = value;
        self.apply_hands_free();
    }

    /// True when hands-free listening is on.
    #[must_use]
    pub fn hands_free(&self) -> bool {
        self.cfg.hands_free && self.cfg.enabled
    }

    /// Replaces the keyword bias list sent with every chunk (agent and project names).
    pub fn set_keywords(&self, keywords: Vec<String>) {
        if let Ok(mut g) = self.inner.lock() {
            g.keywords = keywords;
        }
    }

    /// Push-to-talk down. Stops playback first when the assistant is speaking (barge-in).
    pub fn press(&self) -> Pressed {
        let preroll = self
            .capture
            .as_ref()
            .map(Capture::preroll)
            .unwrap_or_default();
        let speaking = self.is_speaking();
        let now = self.now_ms();
        let pressed = match self.inner.lock() {
            Ok(mut g) => {
                let p = g.ptt.press(now, &preroll, speaking);
                if p.started {
                    g.request = self.next_id.fetch_add(1, Ordering::SeqCst);
                    g.sent = 0;
                }
                p
            }
            Err(_) => Pressed {
                started: false,
                barge_in: false,
            },
        };
        if pressed.barge_in {
            self.stop_speaking();
            tracing::debug!("push-to-talk barge-in: playback stopped");
        }
        pressed
    }

    /// Push-to-talk up; hands the audio to the STT worker.
    pub fn release(&self) -> PttEvent {
        let now = self.now_ms();
        let (finished, id, sent, kw) = match self.inner.lock() {
            Ok(mut g) => {
                let f = g.ptt.release(now);
                (f, g.request, g.sent, g.keywords.clone())
            }
            Err(_) => return PttEvent::Tap,
        };
        match finished {
            Finished::NotRecording | Finished::Tap => PttEvent::Tap,
            Finished::Utterance { pcm, .. } => submit(
                self.stt.as_deref(),
                id,
                &pcm,
                sent,
                &self.cfg.language,
                kw,
                false,
            )
            .unwrap_or(PttEvent::SttUnavailable),
            Finished::TooLong { pcm } => submit(
                self.stt.as_deref(),
                id,
                &pcm,
                sent,
                &self.cfg.language,
                kw,
                true,
            )
            .unwrap_or(PttEvent::SttUnavailable),
        }
    }

    /// Drops an ongoing recording without transcribing.
    pub fn cancel_recording(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.ptt.cancel();
        }
    }

    /// True while a recording is running.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.inner.lock().is_ok_and(|g| g.ptt.is_recording())
    }

    /// Normalised microphone level (`None` when no microphone is open).
    #[must_use]
    pub fn mic_level(&self) -> Option<f32> {
        self.capture.as_ref().map(Capture::level)
    }

    /// Raw RMS of the speaker output.
    #[must_use]
    pub fn output_rms(&self) -> Option<f32> {
        self.playback.as_ref().map(|p| p.output_rms())
    }

    /// True while audio is playing.
    #[must_use]
    pub fn is_speaking(&self) -> bool {
        self.playback.as_ref().is_some_and(|p| p.is_playing())
    }

    /// Queues synthesized audio for playback.
    pub fn play(&self, samples: Vec<f32>, rate: u32) {
        if let Some(p) = &self.playback {
            p.play(samples, rate);
        }
    }

    /// Stops playback now.
    pub fn stop_speaking(&self) {
        if let Some(p) = &self.playback {
            p.stop();
        }
    }

    /// Asks the TTS worker to synthesize `text`; returns the request id.
    pub fn synthesize(&self, text: String) -> Result<u64, SendError> {
        let tts = self.tts.as_ref().ok_or(SendError::Unavailable)?;
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        tts.send(ToWorker::Synth {
            id,
            text,
            voice: self.cfg.tts_voice.clone(),
            lang: Some(self.cfg.language.clone()),
            speed: self.cfg.speed,
        })?;
        Ok(id)
    }

    /// Cancels a synthesis request.
    pub fn cancel_synthesis(&self, id: u64) {
        if let Some(tts) = &self.tts {
            let _ = tts.send(ToWorker::Cancel { id });
        }
    }

    /// State of a worker (`Stopped` when it was never started).
    #[must_use]
    pub fn worker_state(&self, role: Role) -> WorkerState {
        match role {
            Role::Stt => self.stt.as_ref().map(|s| s.state()),
            Role::Tts => self.tts.as_ref().map(Supervisor::state),
        }
        .unwrap_or(WorkerState::Stopped)
    }

    /// The configuration in use.
    #[must_use]
    pub fn config(&self) -> &VoiceConfig {
        &self.cfg
    }
}

fn elapsed_ms(epoch: Instant) -> u64 {
    u64::try_from(epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Sends the unsent tail and the stop marker; `None` when there is no STT worker at all.
fn submit(
    stt: Option<&Supervisor>,
    id: u64,
    pcm: &[f32],
    sent: usize,
    lang: &str,
    keywords: Vec<String>,
    too_long: bool,
) -> Option<PttEvent> {
    let stt = stt?;
    let tail = pcm.get(sent..).unwrap_or(&[]);
    let result = (|| {
        if !tail.is_empty() {
            stt.send(ToWorker::StreamChunk {
                id,
                pcm: tail.to_vec(),
                lang: Some(lang.to_owned()),
                keywords,
            })?;
        }
        stt.send(ToWorker::StreamStop { id })
    })();
    Some(match result {
        Ok(()) => PttEvent::Submitted {
            id,
            samples: pcm.len(),
            too_long,
        },
        Err(SendError::Unavailable) => PttEvent::SttUnavailable,
    })
}

impl Drop for VoiceService {
    fn drop(&mut self) {
        // Capture and Playback stop on drop; the supervisors shut their workers down.
        self.capture = None;
        self.playback = None;
        self.tts = None;
        self.stt = None;
    }
}
