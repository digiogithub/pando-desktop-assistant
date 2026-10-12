#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Digital loopback check of hands-free barge-in with echo cancellation (PDA-T-0139/0140). NOT part of
//! CI: it needs a PipeWire/PulseAudio box, the `pando-stt` / `pando-tts` workers next to the test
//! executables' parent (`cargo build -p pando-stt -p pando-tts`) and a null sink whose monitor is
//! the microphone:
//!
//! ```text
//! pactl load-module module-null-sink sink_name=vmic
//! PANDO_LOOP_IN=pulseaudio:vmic.monitor PANDO_LOOP_OUT=pulseaudio:vmic PANDO_LOOP_SINK=vmic \
//!   cargo test -p app --test live_loop -- --ignored --nocapture
//! ```
//!
//! Phase A: the assistant speaks for a few seconds into the loop; the echo-cleaned signal must not
//! interrupt it. Phase B: a recorded user clip is played into the same loop; the barge-in must stop
//! the playback within the budget.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use app::voice::config::VoiceConfig;
use app::voice::service::{VoiceEvent, VoiceService};
use app::voice::supervisor::{WorkerEvent, WorkerState};
use voice_proto::{FromWorker, Role};

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

#[test]
#[ignore = "needs a loopback sink (see the module docs), the voice workers and the models"]
fn tts_in_the_loop_does_not_interrupt_itself_and_the_user_does() {
    let models = PathBuf::from(env(
        "PANDO_ASSISTANT_MODELS_DIR",
        &format!(
            "{}/.local/share/pando-assistant/models",
            std::env::var("HOME").unwrap_or_default()
        ),
    ));
    let cfg = VoiceConfig {
        enabled: true,
        hands_free: true,
        barge_in: true,
        headset: false,
        input_device: Some(env("PANDO_LOOP_IN", "pulseaudio:vmic.monitor")),
        output_device: Some(env("PANDO_LOOP_OUT", "pulseaudio:vmic")),
        stt_model: Some(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/needle/models/whistle.cact"),
        ),
        tts_model_dir: Some(models.join("piper-es_ES-davefx-medium")),
        ..VoiceConfig::default()
    };
    let (tx, rx) = channel::<VoiceEvent>();
    let tx = std::sync::Mutex::new(tx);
    let tmp = tempfile::tempdir().expect("tmp");
    let svc = VoiceService::start(
        cfg,
        tmp.path(),
        std::sync::Arc::new(move |ev| {
            let _ = tx.lock().unwrap().send(ev);
        }),
    );

    let mut ready = (false, false, false);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut pending = Vec::new();
    while Instant::now() < deadline && !(ready.0 && ready.1 && ready.2) {
        if let Ok(ev) = rx.recv_timeout(Duration::from_millis(100)) {
            match &ev {
                VoiceEvent::Worker(WorkerEvent::State {
                    role: Role::Stt,
                    state: WorkerState::Ready,
                    ..
                }) => ready.0 = true,
                VoiceEvent::Worker(WorkerEvent::State {
                    role: Role::Tts,
                    state: WorkerState::Ready,
                    ..
                }) => ready.1 = true,
                VoiceEvent::Mic(_) => ready.2 = true,
                _ => {}
            }
            pending.push(ev);
        }
    }
    assert!(
        ready.0 && ready.1 && ready.2,
        "voice stack not ready: {ready:?}"
    );
    svc.apply_hands_free();
    std::thread::sleep(Duration::from_secs(1));

    let text = "Las máquinas de vapor cambiaron el mundo. Primero se usaron para bombear agua en las minas. \
                Después movieron telares, barcos y locomotoras. Fue el comienzo de la revolución industrial.";
    svc.synthesize(text.to_owned()).expect("synthesize");
    let (mut barge_a, mut barge_b) = (0, 0);
    let mut first_pcm = None;
    let mut phase_b_at = None;
    let mut stopped_at = None;
    let wav =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../pando-stt/tests/fixtures/oye_pando_es.wav");
    let sink = env("PANDO_LOOP_SINK", "vmic");
    let t_end = Instant::now() + Duration::from_secs(40);
    let mut player = None;
    while Instant::now() < t_end {
        if let Ok(ev) = rx.recv_timeout(Duration::from_millis(5)) {
            match ev {
                VoiceEvent::Worker(WorkerEvent::Message {
                    role: Role::Tts,
                    msg:
                        FromWorker::Pcm {
                            sample_rate,
                            samples,
                            ..
                        },
                }) => {
                    first_pcm.get_or_insert_with(Instant::now);
                    svc.play(samples, sample_rate);
                }
                VoiceEvent::BargeIn => {
                    if phase_b_at.is_some() {
                        barge_b += 1;
                        stopped_at.get_or_insert_with(Instant::now);
                    } else {
                        barge_a += 1;
                    }
                }
                _ => {}
            }
        }
        if let Some(t) = first_pcm
            && phase_b_at.is_none()
            && t.elapsed() > Duration::from_secs(4)
        {
            println!("phase A: {barge_a} barge-ins in 4 s of TTS echo");
            assert_eq!(barge_a, 0, "the assistant interrupted itself");
            phase_b_at = Some(Instant::now());
            player = Some(
                Command::new("paplay")
                    .arg(format!("--device={sink}"))
                    .arg(&wav)
                    .spawn()
                    .expect("paplay"),
            );
        }
        if let (Some(t0), Some(t1)) = (phase_b_at, stopped_at) {
            println!(
                "phase B: user speech -> playback stopped after {:?}",
                t1 - t0
            );
            break;
        }
    }
    if let Some(mut p) = player {
        let _ = p.wait();
    }
    assert!(barge_b >= 1, "the user did not interrupt the assistant");
    let t_stop = Instant::now();
    while svc.output_rms().unwrap_or(0.0) > 0.0 && t_stop.elapsed() < Duration::from_secs(1) {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!svc.is_speaking());
}
