#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stdout)]
//! Hands-free wake word through a virtual microphone (PDA-T-0138). NOT part of CI:
//!
//! ```text
//! pactl load-module module-null-sink sink_name=vmic
//! PANDO_LOOP_IN=pulseaudio:vmic.monitor PANDO_LOOP_SINK=vmic \
//!   cargo test --release -p app --test live_wake -- --ignored --nocapture
//! ```
//!
//! Plays every `PANDO_WAKE_STEP`-th positive clip of the S5 synthetic set (`list_pos.tsv` in
//! `PANDO_WAKE_DIR`, default `~/.cache/pando-assistant-spikes/wake`) and the first confusers into
//! the sink and counts the `WakeWord` events.

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

fn clips(dir: &Path, list: &str, step: usize, max: usize) -> Vec<(PathBuf, String)> {
    let text = std::fs::read_to_string(dir.join(list)).expect("list");
    text.lines()
        .step_by(step)
        .take(max)
        .filter_map(|l| {
            let mut c = l.split('\t');
            let _id = c.next()?;
            Some((PathBuf::from(c.next()?), c.next()?.to_owned()))
        })
        .collect()
}

#[test]
#[ignore = "needs a null-sink loop (see the module docs), pando-stt and the S5 clips"]
fn wake_word_through_the_virtual_mic() {
    let cfg = VoiceConfig {
        enabled: true,
        hands_free: true,
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
    svc.apply_hands_free();
    std::thread::sleep(Duration::from_millis(800));
    let dir = PathBuf::from(env(
        "PANDO_WAKE_DIR",
        &format!(
            "{}/.cache/pando-assistant-spikes/wake",
            std::env::var("HOME").unwrap_or_default()
        ),
    ));
    let step: usize = env("PANDO_WAKE_STEP", "20").parse().unwrap();
    let sink = env("PANDO_LOOP_SINK", "vmic");
    let run = |label: &str, list: &str, max: usize| {
        let mut woke = 0;
        let set = clips(&dir, list, step, max);
        for (path, phrase) in &set {
            while rx.try_recv().is_ok() {}
            let status = Command::new("paplay")
                .arg(format!("--device={sink}"))
                .arg(path)
                .status()
                .expect("paplay");
            assert!(status.success());
            let t0 = Instant::now();
            let mut hit = false;
            let mut heard = Vec::new();
            while t0.elapsed() < Duration::from_millis(3000) {
                if let Ok(VoiceEvent::Worker(WorkerEvent::Message {
                    role: Role::Stt,
                    msg,
                })) = rx.recv_timeout(Duration::from_millis(20))
                {
                    match msg {
                        FromWorker::WakeWord { .. } => hit = true,
                        FromWorker::Utterance { text, .. } => heard.push(text),
                        _ => {}
                    }
                }
            }
            if hit && heard.is_empty() {
                // a bare wake phrase opens an 8 s command window: let it expire
                std::thread::sleep(Duration::from_millis(8_200));
            }
            woke += usize::from(hit);
            println!(
                "{label} {} {phrase:?}: woke={hit} heard={heard:?}",
                path.file_name().unwrap().to_string_lossy()
            );
        }
        println!("{label}: {woke}/{} woke", set.len());
    };
    run(
        "positive",
        "list_pos.tsv",
        env("PANDO_WAKE_MAX", "9").parse().unwrap(),
    );
    run("confuser", "list_conf.tsv", 9);
}
