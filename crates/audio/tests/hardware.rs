#![allow(clippy::expect_used)]
//! Needs a sound card (default input and output). Run:
//! `cargo test -p audio --test hardware -- --ignored --nocapture --test-threads=1`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use audio::{Capture, CaptureConfig, CaptureEvent, Playback, PlaybackConfig, PlaybackEvent};

fn tone(rate: u32, secs: f32) -> Vec<f32> {
    (0..(rate as f32 * secs) as usize)
        .map(|i| 0.05 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin())
        .collect()
}

fn wait(mut cond: impl FnMut() -> bool, max: Duration) -> bool {
    let t = Instant::now();
    while t.elapsed() < max {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    cond()
}

#[test]
#[ignore = "needs an audio output device"]
fn playback_events_rms_and_stop() {
    let events = Arc::new(Mutex::new(Vec::<PlaybackEvent>::new()));
    let e2 = events.clone();
    let pb = Playback::start(PlaybackConfig::default(), move |e| {
        if let Ok(mut v) = e2.lock() {
            v.push(e);
        }
    });
    let has = |p: fn(&PlaybackEvent) -> bool| events.lock().expect("lock").iter().any(p);
    assert!(wait(
        || has(|e| matches!(e, PlaybackEvent::DeviceReady { .. })),
        Duration::from_secs(5)
    ));

    // 0.4 s tone at the Piper rate, played to the end.
    pb.play(tone(22_050, 0.4), 22_050);
    assert!(wait(
        || has(|e| matches!(e, PlaybackEvent::Started)),
        Duration::from_secs(2)
    ));
    let mut max_rms = 0.0f32;
    assert!(wait(
        || {
            max_rms = max_rms.max(pb.output_rms());
            has(|e| matches!(e, PlaybackEvent::Drained))
        },
        Duration::from_secs(3)
    ));
    assert!(max_rms > 0.02, "output RMS never rose: {max_rms}");
    // The PulseAudio backend's default 2 s buffer would show up here and delay barge-in.
    assert!(
        pb.output_latency() < Duration::from_millis(250),
        "output latency {:?}",
        pb.output_latency()
    );
    println!(
        "max rms {max_rms:.3}, reported output latency {:?}",
        pb.output_latency()
    );

    // 5 s of audio, stopped after 100 ms: silence within a few buffers.
    events.lock().expect("lock").clear();
    pb.play(tone(22_050, 5.0), 22_050);
    assert!(wait(
        || has(|e| matches!(e, PlaybackEvent::Started)),
        Duration::from_secs(2)
    ));
    std::thread::sleep(Duration::from_millis(100));
    let t = Instant::now();
    pb.stop();
    assert!(wait(|| pb.output_rms() == 0.0, Duration::from_secs(1)));
    let stop_ms = t.elapsed().as_millis();
    println!("stop -> silent callback: {stop_ms} ms");
    assert!(stop_ms < 50, "stop took {stop_ms} ms");
    assert!(wait(
        || has(|e| matches!(e, PlaybackEvent::Drained)),
        Duration::from_secs(1)
    ));
    assert!(!pb.is_playing());
}

#[test]
#[ignore = "needs an audio input device"]
fn capture_delivers_16k_frames_and_preroll() {
    let frames = Arc::new(AtomicUsize::new(0));
    let events = Arc::new(Mutex::new(Vec::<CaptureEvent>::new()));
    let (f2, e2) = (frames.clone(), events.clone());
    let cap = Capture::start(
        CaptureConfig::default(),
        move |f| {
            assert_eq!(f.len(), 320);
            f2.fetch_add(1, Ordering::Relaxed);
        },
        move |e| {
            if let Ok(mut v) = e2.lock() {
                v.push(e);
            }
        },
    );
    let t0 = Instant::now();
    assert!(wait(
        || events
            .lock()
            .expect("lock")
            .iter()
            .any(|e| matches!(e, CaptureEvent::Started { .. })),
        Duration::from_secs(10)
    ));
    println!("stream started after {} ms", t0.elapsed().as_millis());
    frames.store(0, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(1500));
    let n = frames.load(Ordering::Relaxed);
    println!(
        "{n} frames in 1.5 s; events {:?}; level {}",
        events.lock().expect("lock"),
        cap.level()
    );
    assert!((60..=100).contains(&n), "expected about 75 frames, got {n}");
    assert!(cap.preroll().len() >= 16_000);
    assert_eq!(cap.restarts(), 0);
}

/// Soak: `PANDO_AUDIO_SOAK_SECS` (default 60) of capture must deliver every frame and never
/// restart. With `PANDO_AUDIO_SUSPEND_SOURCE=<pulse source name>` (Linux, needs `pactl`) the
/// source is suspended mid-run, as a stalled device, to exercise the watchdog.
#[test]
#[ignore = "needs an audio input device; takes a minute"]
fn capture_soak_and_stall_watchdog() {
    let secs: u64 = std::env::var("PANDO_AUDIO_SOAK_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let frames = Arc::new(AtomicUsize::new(0));
    let events = Arc::new(Mutex::new(Vec::<CaptureEvent>::new()));
    let (f2, e2) = (frames.clone(), events.clone());
    let cap = Capture::start(
        CaptureConfig::default(),
        move |_| {
            f2.fetch_add(1, Ordering::Relaxed);
        },
        move |e| {
            if let Ok(mut v) = e2.lock() {
                v.push(e);
            }
        },
    );
    assert!(wait(
        || cap.level() >= 0.0 && frames.load(Ordering::Relaxed) > 0,
        Duration::from_secs(10)
    ));
    let t0 = Instant::now();
    let f0 = frames.load(Ordering::Relaxed);
    let src = std::env::var("PANDO_AUDIO_SUSPEND_SOURCE").ok();
    let mut suspended = false;
    while t0.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_millis(200));
        if let (Some(s), false) = (&src, suspended)
            && t0.elapsed() > Duration::from_secs(secs / 2)
        {
            suspended = true;
            let ok = std::process::Command::new("pactl")
                .args(["suspend-source", s, "1"])
                .status()
                .is_ok_and(|st| st.success());
            println!("suspend-source {s}: {ok}");
            std::thread::sleep(Duration::from_secs(3));
            let _ = std::process::Command::new("pactl")
                .args(["suspend-source", s, "0"])
                .status();
            println!(
                "after resume: restarts {}, frames {}",
                cap.restarts(),
                frames.load(Ordering::Relaxed)
            );
        }
    }
    let n = frames.load(Ordering::Relaxed) - f0;
    println!(
        "{n} frames in {:.1} s (50/s nominal), restarts {}, events {:?}",
        t0.elapsed().as_secs_f64(),
        cap.restarts(),
        events.lock().expect("lock")
    );
    if src.is_none() {
        let nominal = (t0.elapsed().as_secs_f64() * 50.0) as usize;
        assert!(
            n.abs_diff(nominal) < nominal / 20,
            "{n} frames vs {nominal}"
        );
        assert_eq!(cap.restarts(), 0);
    }
}
