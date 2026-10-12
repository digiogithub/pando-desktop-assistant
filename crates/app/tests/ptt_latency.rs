#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stdout)]
//! Push-to-talk release -> final transcript latency on real audio (PDA-T-0095). NOT part of CI:
//!
//! ```text
//! pactl load-module module-null-sink sink_name=vmic
//! PANDO_LOOP_IN=pulseaudio:vmic.monitor PANDO_LOOP_SINK=vmic \
//!   cargo test --release -p app --test ptt_latency -- --ignored --nocapture
//! ```
//!
//! A recorded clip is played into the null sink while the key is held; the key is released right
//! after the clip ends and the time to the worker's final `Transcript` is printed (P50/P95).

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
#[ignore = "needs a null-sink loop (see the module docs), pando-stt and the Whistle model"]
fn release_to_final_transcript_latency() {
    let cfg = VoiceConfig {
        enabled: true,
        input_device: Some(env("PANDO_LOOP_IN", "pulseaudio:vmic.monitor")),
        stt_model: Some(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/needle/models/whistle.cact"),
        ),
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
    let (mut stt, mut mic) = (false, false);
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline && !(stt && mic) {
        if let Ok(ev) = rx.recv_timeout(Duration::from_millis(100)) {
            match ev {
                VoiceEvent::Worker(WorkerEvent::State {
                    role: Role::Stt,
                    state: WorkerState::Ready,
                    ..
                }) => stt = true,
                VoiceEvent::Mic(_) => mic = true,
                _ => {}
            }
        }
    }
    assert!(stt && mic, "voice stack not ready");
    std::thread::sleep(Duration::from_millis(500));
    let wav: PathBuf = env("PANDO_PTT_WAV", "").into();
    let wav = if wav.as_os_str().is_empty() {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../pando-stt/tests/fixtures/oye_pando_es.wav")
    } else {
        wav
    };
    let sink = env("PANDO_LOOP_SINK", "vmic");
    let rounds: usize = env("PANDO_PTT_ROUNDS", "12").parse().unwrap();
    // Time between the end of the clip and the key release (a real user releases within ~100 ms).
    let hold_ms: u64 = env("PANDO_PTT_HOLD_MS", "0").parse().unwrap();
    let mut lat = Vec::new();
    for round in 0..rounds {
        while rx.try_recv().is_ok() {}
        svc.press();
        let status = Command::new("paplay")
            .arg(format!("--device={sink}"))
            .arg(&wav)
            .status()
            .expect("paplay");
        assert!(status.success());
        std::thread::sleep(Duration::from_millis(hold_ms));
        let t0 = Instant::now();
        svc.release();
        let mut text = None;
        while t0.elapsed() < Duration::from_secs(5) {
            if let Ok(VoiceEvent::Worker(WorkerEvent::Message {
                role: Role::Stt,
                msg: FromWorker::Transcript { text: t, .. },
            })) = rx.recv_timeout(Duration::from_millis(1))
            {
                text = Some(t);
                break;
            }
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("round {round}: {ms:.0} ms {text:?}");
        if round > 0 && text.is_some() {
            lat.push(ms); // round 0 warms the model
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    lat.sort_by(f64::total_cmp);
    let pct = |p: f64| lat[((lat.len() as f64 * p).ceil() as usize).clamp(1, lat.len()) - 1];
    println!(
        "release -> final transcript: n={} P50={:.0} ms P95={:.0} ms min={:.0}",
        lat.len(),
        pct(0.5),
        pct(0.95),
        lat[0]
    );
}
