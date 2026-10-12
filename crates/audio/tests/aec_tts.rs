#![cfg(feature = "echo-cancel")]
#![allow(clippy::expect_used, clippy::chunks_exact_to_as_chunks)]
//! PDA-T-0139: five minutes of real TTS speech played "through speakers" (synthetic room echo on the
//! microphone) must leave a residual too quiet for any VAD to fire, with ERLE above the S4 floor.
//! Needs `~/.cache/pando-assistant-spikes/s3-samples/piper_t4.wav` (Piper paragraph).
//! Run: `cargo test -p audio --features echo-cancel --test aec_tts -- --ignored --nocapture`.

use audio::aec::{AEC_FRAME, EchoProcessor, SonoraAec};

fn db(x: &[f32]) -> f32 {
    let e = x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64;
    (10.0 * e.max(1e-12).log10()) as f32
}

#[test]
#[ignore = "needs the Piper sample from the spikes cache"]
fn five_minutes_of_tts_leave_no_residual() {
    let path = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join(".cache/pando-assistant-spikes/s3-samples/piper_t4.wav");
    let mut r = hound::WavReader::open(path).expect("sample");
    let spec = r.spec();
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.expect("s")).collect(),
        hound::SampleFormat::Int => r
            .samples::<i16>()
            .map(|s| f32::from(s.expect("s")) / 32768.0)
            .collect(),
    };
    let clip = audio::resample::resample_all(&raw, spec.sample_rate, 16_000).expect("resample");
    let mut refr = Vec::new();
    while refr.len() < 16_000 * 300 {
        refr.extend_from_slice(&clip);
        refr.extend(std::iter::repeat_n(0.0, 8000)); // 0.5 s pause between utterances
    }
    refr.truncate(16_000 * 300);
    // speaker -> room -> mic at 10 dB ERL: 60 ms bulk delay plus a decaying tail, mic noise floor
    let g = 10f32.powf(-10.0 / 20.0);
    let taps: [(usize, f32); 5] = [
        (960, 1.0),
        (1100, 0.5),
        (1400, -0.3),
        (2000, 0.2),
        (3200, -0.1),
    ];
    let mut seed = 1u32;
    let mic: Vec<f32> = (0..refr.len())
        .map(|i| {
            let a: f32 = taps
                .iter()
                .filter(|(d, _)| i >= *d)
                .map(|(d, c)| c * refr[i - d])
                .sum();
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            g * a + ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.0004
        })
        .collect();
    let mut aec = SonoraAec::new(true);
    let mut out = mic.clone();
    for (rc, mc) in refr
        .chunks_exact(AEC_FRAME)
        .zip(out.chunks_exact_mut(AEC_FRAME))
    {
        let mut rf = [0.0; AEC_FRAME];
        rf.copy_from_slice(rc);
        aec.render(&rf);
        let mut cf = [0.0; AEC_FRAME];
        cf.copy_from_slice(mc);
        aec.capture(&mut cf);
        mc.copy_from_slice(&cf);
    }
    // after the 10 s convergence: worst 100 ms block of the residual and overall ERLE
    let (mut pin, mut pout, mut worst) = (0.0f64, 0.0f64, -120.0f32);
    for b in (16_000 * 10..out.len() - 1600).step_by(1600) {
        pin += mic[b..b + 1600]
            .iter()
            .map(|v| (*v as f64).powi(2))
            .sum::<f64>();
        pout += out[b..b + 1600]
            .iter()
            .map(|v| (*v as f64).powi(2))
            .sum::<f64>();
        worst = worst.max(db(&out[b..b + 1600]));
    }
    let erle = (10.0 * (pin / pout.max(1e-12)).log10()) as f32;
    println!(
        "mic {:.1} dBFS, residual {:.1} dBFS, worst 100 ms block {worst:.1} dBFS, ERLE {erle:.1} dB",
        db(&mic),
        db(&out)
    );
    assert!(erle >= 20.0, "ERLE {erle}");
    // an energy VAD needs > -50 dBFS (EnergyConfig::min_level_db); stay well below
    assert!(worst < -50.0, "residual peaks at {worst} dBFS");
}
