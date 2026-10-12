//! Echo cancellation for hands-free mode (PDA-T-0139, `PDA-SP-0011.R4`, spike S4).
//!
//! sonora AEC3 (+ noise suppression) at 16 kHz mono in 10 ms frames. The render reference is the
//! exact PCM written by the output callback ([`crate::ReferenceTap`]), resampled to 16 kHz; the capture
//! side is the 16 kHz microphone stream, processed before VAD and wake detection. With a headset
//! the canceller is bypassed. [`EchoProcessor`] is the seam to swap in `webrtc-audio-processing`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use sonora::config::{EchoCanceller, NoiseSuppression};
use sonora::{AudioProcessing, Config, StreamConfig};

use crate::playback::ReferenceTap;
use crate::resample::StreamResampler;

/// One 10 ms frame at 16 kHz.
pub const AEC_FRAME: usize = 160;

/// A 16 kHz mono echo canceller working in 10 ms frames.
pub trait EchoProcessor: Send {
    /// Far-end (played) frame.
    fn render(&mut self, frame: &[f32; AEC_FRAME]);
    /// Near-end (microphone) frame, cleaned in place.
    fn capture(&mut self, frame: &mut [f32; AEC_FRAME]);
    /// Echo return loss enhancement reported by the engine (dB), if known.
    fn erle_db(&self) -> Option<f32> {
        None
    }
}

/// sonora AEC3 + noise suppression.
pub struct SonoraAec {
    apm: AudioProcessing,
    scratch: [f32; AEC_FRAME],
}

impl std::fmt::Debug for SonoraAec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SonoraAec")
    }
}

impl SonoraAec {
    pub fn new(noise_suppression: bool) -> Self {
        let sc = StreamConfig::new(16_000, 1);
        let apm = AudioProcessing::builder()
            .config(Config {
                echo_canceller: Some(EchoCanceller::default()),
                noise_suppression: noise_suppression.then(NoiseSuppression::default),
                ..Default::default()
            })
            .capture_config(sc)
            .render_config(sc)
            .build();
        Self {
            apm,
            scratch: [0.0; AEC_FRAME],
        }
    }
}

impl EchoProcessor for SonoraAec {
    fn render(&mut self, frame: &[f32; AEC_FRAME]) {
        let _ = self
            .apm
            .process_render_f32(&[&frame[..]], &mut [&mut self.scratch[..]]);
    }

    fn capture(&mut self, frame: &mut [f32; AEC_FRAME]) {
        let src = *frame;
        let _ = self
            .apm
            .process_capture_f32(&[&src[..]], &mut [&mut frame[..]]);
    }

    fn erle_db(&self) -> Option<f32> {
        self.apm
            .statistics()
            .echo_return_loss_enhancement
            .map(|v| v as f32)
    }
}

/// Device names that indicate the sound does not reach the microphone (no echo to cancel).
pub fn looks_like_headset(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "headset",
        "headphone",
        "earbud",
        "earphone",
        "airpods",
        "bluez",
        "bluetooth",
        "hands-free",
    ]
    .iter()
    .any(|k| n.contains(k))
}

/// Echo canceller + the plumbing that feeds it the played reference.
pub struct EchoCancel {
    tap: ReferenceTap,
    proc: Box<dyn EchoProcessor>,
    resampler: Option<StreamResampler>,
    tap_rate: u32,
    native: Vec<f32>,
    render: VecDeque<f32>,
    active: bool,
}

/// Clonable handle stored in `CaptureConfig`.
pub type EchoHandle = Arc<Mutex<EchoCancel>>;

impl std::fmt::Debug for EchoCancel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EchoCancel(active {})", self.active)
    }
}

impl EchoCancel {
    pub fn new(tap: ReferenceTap, proc: Box<dyn EchoProcessor>) -> Self {
        Self {
            tap,
            proc,
            resampler: None,
            tap_rate: 0,
            native: Vec::new(),
            render: VecDeque::new(),
            active: true,
        }
    }

    /// sonora AEC3 + NS reading `tap`.
    pub fn sonora(tap: ReferenceTap) -> EchoHandle {
        Arc::new(Mutex::new(Self::new(tap, Box::new(SonoraAec::new(true)))))
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Bypass the canceller (headset) or re-enable it.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn erle_db(&self) -> Option<f32> {
        self.proc.erle_db()
    }

    /// Pull the newly played audio, resample it to 16 kHz and queue it.
    fn pull_reference(&mut self) {
        self.native.clear();
        let Some(rate) = self.tap.drain_into(&mut self.native) else {
            return;
        };
        if rate != self.tap_rate {
            self.tap_rate = rate;
            self.resampler = StreamResampler::new(rate, crate::CAPTURE_RATE).ok();
            self.render.clear();
        }
        if let Some(r) = self.resampler.as_mut() {
            let mut out = Vec::with_capacity(self.native.len() / 2 + 16);
            if r.process(&self.native, &mut out).is_ok() {
                self.render.extend(out);
            }
        }
        // Keep at most 300 ms of backlog: the reference must stay close to the capture clock.
        let max = 16_000 * 3 / 10;
        if self.render.len() > max {
            let drop = self.render.len() - max;
            self.render.drain(..drop);
        }
    }

    /// Clean a capture block in place (any multiple of 10 ms is processed; a remainder is left
    /// untouched).
    pub fn process(&mut self, capture: &mut [f32]) {
        if !self.active {
            self.render.clear();
            let mut sink = Vec::new();
            let _ = self.tap.drain_into(&mut sink); // keep the ring from filling up
            return;
        }
        self.pull_reference();
        #[allow(clippy::chunks_exact_to_as_chunks)]
        for block in capture.chunks_exact_mut(AEC_FRAME) {
            let mut r = [0.0f32; AEC_FRAME];
            for s in r.iter_mut() {
                *s = self.render.pop_front().unwrap_or(0.0);
            }
            self.proc.render(&r);
            let mut c = [0.0f32; AEC_FRAME];
            c.copy_from_slice(block);
            self.proc.capture(&mut c);
            block.copy_from_slice(&c);
        }
    }
}

#[cfg(test)]
#[allow(clippy::chunks_exact_to_as_chunks)]
mod tests {
    use super::*;

    fn rms_db(x: &[f32]) -> f32 {
        let e = x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64;
        (10.0 * e.max(1e-12).log10()) as f32
    }

    /// Speech-like far-end: bursts of band-limited noise with pauses.
    fn far_end(n: usize) -> Vec<f32> {
        let mut seed = 12345u32;
        let mut lp = 0.0f32;
        (0..n)
            .map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let r = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                lp += 0.35 * (r - lp);
                let env = if (i / 8000) % 3 == 2 { 0.0 } else { 1.0 };
                lp * 1.2 * env
            })
            .collect()
    }

    /// Echo through a 60 ms bulk delay and a short decaying room response, plus a mic noise floor.
    fn echo_of(refr: &[f32], erl_db: f32) -> Vec<f32> {
        let g = 10f32.powf(-erl_db / 20.0);
        let taps: [(usize, f32); 4] = [(960, 1.0), (1100, 0.5), (1400, -0.3), (2000, 0.2)];
        let mut seed = 777u32;
        (0..refr.len())
            .map(|i| {
                let mut a = 0.0;
                for (d, c) in taps {
                    if i >= d {
                        a += c * refr[i - d];
                    }
                }
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.0004;
                g * a + noise
            })
            .collect()
    }

    fn run_erle(erl: f32) -> f32 {
        let n = 16_000 * 20;
        let refr = far_end(n);
        let mic = echo_of(&refr, erl);
        let mut aec = SonoraAec::new(false);
        let mut out = mic.clone();
        for (r, c) in refr
            .chunks_exact(AEC_FRAME)
            .zip(out.chunks_exact_mut(AEC_FRAME))
        {
            let mut rf = [0.0; AEC_FRAME];
            rf.copy_from_slice(r);
            aec.render(&rf);
            let mut cf = [0.0; AEC_FRAME];
            cf.copy_from_slice(c);
            aec.capture(&mut cf);
            c.copy_from_slice(&cf);
        }
        // ERLE over the last 10 s, blocks where the far end is active
        let (mut pin, mut pout) = (0.0f64, 0.0f64);
        for b in (16_000 * 10..n - 1600).step_by(1600) {
            if rms_db(&refr[b..b + 1600]) > -40.0 {
                pin += mic[b..b + 1600]
                    .iter()
                    .map(|v| (*v as f64).powi(2))
                    .sum::<f64>();
                pout += out[b..b + 1600]
                    .iter()
                    .map(|v| (*v as f64).powi(2))
                    .sum::<f64>();
            }
        }
        (10.0 * (pin / pout.max(1e-12)).log10()) as f32
    }

    #[test]
    fn erle_meets_the_spike_s4_target() {
        // S4 measured 23-31 dB on a synthetic echo path; require >= 20 dB for ERL 10 and 20 dB.
        for erl in [10.0, 20.0] {
            let erle = run_erle(erl);
            println!("ERL {erl} dB -> ERLE {erle:.1} dB");
            assert!(erle >= 20.0, "ERL {erl}: ERLE {erle:.1} dB");
        }
    }

    #[test]
    fn plumbing_cancels_the_played_reference_end_to_end() {
        // A producer mimics the output callback; EchoCancel pulls, resamples (48 kHz -> 16 kHz) and cancels.
        let tap = ReferenceTap::new();
        let (mut prod, cons) = rtrb::RingBuffer::<f32>::new(48_000);
        tap.install(cons, 48_000);
        let mut ec = EchoCancel::new(tap, Box::new(SonoraAec::new(false)));
        let far16 = far_end(16_000 * 20);
        let far48 = crate::resample::resample_all(&far16, 16_000, 48_000).expect("up");
        let mic16 = echo_of(&far16, 12.0);
        let mut out = Vec::new();
        let mut fed = 0;
        for (i, chunk) in mic16.chunks(320).enumerate() {
            // 20 ms of played audio reaches the tap, then the matching mic block is processed
            let want = ((i + 1) * 960).min(far48.len());
            while fed < want {
                let _ = prod.push(far48[fed]);
                fed += 1;
            }
            let mut block = chunk.to_vec();
            ec.process(&mut block);
            out.extend(block);
        }
        let (mut pin, mut pout) = (0.0f64, 0.0f64);
        for b in (16_000 * 10..mic16.len() - 1600).step_by(1600) {
            if rms_db(&far16[b..b + 1600]) > -40.0 {
                pin += mic16[b..b + 1600]
                    .iter()
                    .map(|v| (*v as f64).powi(2))
                    .sum::<f64>();
                pout += out[b..b + 1600]
                    .iter()
                    .map(|v| (*v as f64).powi(2))
                    .sum::<f64>();
            }
        }
        let erle = (10.0 * (pin / pout.max(1e-12)).log10()) as f32;
        println!("end-to-end ERLE {erle:.1} dB");
        assert!(erle >= 15.0, "{erle}");
    }

    #[test]
    fn bypass_leaves_the_signal_untouched() {
        let mut ec = EchoCancel::new(ReferenceTap::new(), Box::new(SonoraAec::new(true)));
        ec.set_active(false);
        let mut x: Vec<f32> = (0..640).map(|i| (i as f32 * 0.01).sin() * 0.2).collect();
        let before = x.clone();
        ec.process(&mut x);
        assert_eq!(x, before);
    }

    #[test]
    fn headset_names() {
        assert!(looks_like_headset("WH-1000XM4 Hands-Free"));
        assert!(looks_like_headset("bluez_input.00_1B"));
        assert!(looks_like_headset("USB Headset"));
        assert!(!looks_like_headset("LCS USB Audio Mono"));
        assert!(!looks_like_headset("Built-in Audio Analog Stereo"));
    }
}
