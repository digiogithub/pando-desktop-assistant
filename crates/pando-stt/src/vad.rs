//! Voice activity detection and segmentation for hands-free mode (PDA-T-0136, `PDA-SP-0011.R1`).
//!
//! Audio is processed in 32 ms frames (512 samples at 16 kHz). Nothing is stored: segments live in
//! memory until they are handed to the wake verifier. Two implementations of [`SpeechSegmenter`]:
//! [`EnergySegmenter`] (pure Rust, no model, the default) and `SileroSegmenter` (feature
//! `silero-vad`, Silero v5 through the TTS-off sherpa-onnx build).

use std::collections::VecDeque;

/// Frame size: 32 ms at 16 kHz.
pub const FRAME: usize = 512;
const RATE: usize = 16_000;

/// A speech segment cut out of the stream.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// Index of the first sample in the stream.
    pub start: u64,
    pub samples: Vec<f32>,
}

impl Segment {
    pub fn seconds(&self) -> f32 {
        self.samples.len() as f32 / RATE as f32
    }
}

/// What the segmenter reports.
#[derive(Debug, Clone, PartialEq)]
pub enum VadEvent {
    /// Speech started (reported once the onset is confirmed).
    Start,
    /// Speech ended; the whole segment including pre-roll and hangover.
    End(Segment),
}

/// Turns a continuous stream into speech segments.
pub trait SpeechSegmenter {
    /// Feed any amount of audio; events are appended to `out`.
    fn push(&mut self, pcm: &[f32], out: &mut Vec<VadEvent>);
    /// End of stream: close an open segment.
    fn flush(&mut self, out: &mut Vec<VadEvent>);
}

/// Tunables of [`EnergySegmenter`].
#[derive(Debug, Clone, Copy)]
pub struct EnergyConfig {
    /// Speech is this many dB above the tracked noise floor.
    pub margin_db: f32,
    /// Never treat anything quieter than this (dBFS) as speech.
    pub min_level_db: f32,
    /// Consecutive speech frames that confirm an onset (3 = 96 ms).
    pub onset_frames: usize,
    /// Silence that closes a segment, in frames (19 = 608 ms).
    pub hangover_frames: usize,
    /// Pre-roll kept before the onset, in frames (10 = 320 ms >= 300 ms).
    pub preroll_frames: usize,
    /// Segments with less speech than this are dropped (clicks), in frames.
    pub min_speech_frames: usize,
    /// Hard limit; a longer segment is cut and a new one starts.
    pub max_segment_frames: usize,
}

impl Default for EnergyConfig {
    fn default() -> Self {
        Self {
            margin_db: 10.0,
            min_level_db: -50.0,
            onset_frames: 3,
            hangover_frames: 19,
            preroll_frames: 10,
            min_speech_frames: 8,
            max_segment_frames: 30 * RATE / FRAME,
        }
    }
}

/// Adaptive-threshold energy VAD with pre-roll and hangover.
#[derive(Debug)]
pub struct EnergySegmenter {
    cfg: EnergyConfig,
    pending: Vec<f32>,
    floor_db: f32,
    warmup: usize,
    preroll: VecDeque<Vec<f32>>,
    consumed: u64,
    state: State,
}

#[derive(Debug)]
enum State {
    Idle {
        run: usize,
    },
    Speech {
        start: u64,
        samples: Vec<f32>,
        silence: usize,
        speech_frames: usize,
    },
}

fn level_db(frame: &[f32]) -> f32 {
    let e = frame.iter().map(|s| s * s).sum::<f32>() / frame.len().max(1) as f32;
    10.0 * e.max(1e-12).log10()
}

impl EnergySegmenter {
    pub fn new(cfg: EnergyConfig) -> Self {
        Self {
            cfg,
            pending: Vec::with_capacity(2 * FRAME),
            floor_db: -60.0,
            warmup: 20,
            preroll: VecDeque::new(),
            consumed: 0,
            state: State::Idle { run: 0 },
        }
    }

    fn frame(&mut self, frame: Vec<f32>, out: &mut Vec<VadEvent>) {
        let lvl = level_db(&frame);
        let threshold = (self.floor_db + self.cfg.margin_db).max(self.cfg.min_level_db);
        let is_speech = lvl > threshold;
        // The noise floor follows quiet frames only (and everything during warm-up).
        if self.warmup > 0 {
            self.warmup -= 1;
            self.floor_db += 0.2 * (lvl - self.floor_db);
        } else if !is_speech {
            self.floor_db += 0.05 * (lvl - self.floor_db);
        }
        let frame_start = self.consumed;
        self.consumed += FRAME as u64;
        let cfg = self.cfg;
        match &mut self.state {
            State::Idle { run } => {
                self.preroll.push_back(frame);
                // keep the onset frames plus the pre-roll
                while self.preroll.len() > cfg.preroll_frames + cfg.onset_frames {
                    self.preroll.pop_front();
                }
                if is_speech {
                    *run += 1;
                } else {
                    *run = 0;
                }
                if *run >= cfg.onset_frames {
                    let samples: Vec<f32> = self.preroll.drain(..).flatten().collect();
                    let start = frame_start + FRAME as u64 - samples.len() as u64;
                    out.push(VadEvent::Start);
                    self.state = State::Speech {
                        start,
                        samples,
                        silence: 0,
                        speech_frames: cfg.onset_frames,
                    };
                }
            }
            State::Speech {
                samples,
                silence,
                speech_frames,
                ..
            } => {
                samples.extend_from_slice(&frame);
                if is_speech {
                    *silence = 0;
                    *speech_frames += 1;
                } else {
                    *silence += 1;
                }
                let too_long = samples.len() >= cfg.max_segment_frames * FRAME;
                if *silence >= cfg.hangover_frames || too_long {
                    self.close(out);
                }
            }
        }
    }

    fn close(&mut self, out: &mut Vec<VadEvent>) {
        if let State::Speech {
            start,
            samples,
            speech_frames,
            ..
        } = std::mem::replace(&mut self.state, State::Idle { run: 0 })
            && speech_frames >= self.cfg.min_speech_frames
        {
            out.push(VadEvent::End(Segment { start, samples }));
        }
    }
}

impl Default for EnergySegmenter {
    fn default() -> Self {
        Self::new(EnergyConfig::default())
    }
}

impl SpeechSegmenter for EnergySegmenter {
    fn push(&mut self, pcm: &[f32], out: &mut Vec<VadEvent>) {
        self.pending.extend_from_slice(pcm);
        let mut offset = 0;
        while self.pending.len() - offset >= FRAME {
            let frame = self.pending[offset..offset + FRAME].to_vec();
            offset += FRAME;
            self.frame(frame, out);
        }
        self.pending.drain(..offset);
    }

    fn flush(&mut self, out: &mut Vec<VadEvent>) {
        if !self.pending.is_empty() {
            let mut last = std::mem::take(&mut self.pending);
            last.resize(FRAME, 0.0);
            self.frame(last, out);
        }
        self.close(out);
    }
}

#[cfg(feature = "silero-vad")]
pub use silero::SileroSegmenter;

/// Silero VAD v5 through sherpa-onnx (must be the TTS-off source build, ADR 0007).
#[cfg(feature = "silero-vad")]
mod silero {
    use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};

    use super::{Segment, SpeechSegmenter, VadEvent};
    use crate::stt::SttError;

    pub struct SileroSegmenter {
        vad: VoiceActivityDetector,
        in_speech: bool,
    }

    impl std::fmt::Debug for SileroSegmenter {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("SileroSegmenter")
        }
    }

    impl SileroSegmenter {
        /// `model`: path of `silero_vad.onnx` (v5). 32 ms windows, 0.3 s min silence, 0.25 s min speech.
        pub fn new(model: &std::path::Path) -> Result<Self, SttError> {
            let cfg = VadModelConfig {
                silero_vad: SileroVadModelConfig {
                    model: Some(model.to_string_lossy().into_owned()),
                    threshold: 0.5,
                    min_silence_duration: 0.3,
                    min_speech_duration: 0.25,
                    window_size: 512,
                    max_speech_duration: 30.0,
                },
                sample_rate: 16_000,
                num_threads: 1,
                provider: Some("cpu".into()),
                ..Default::default()
            };
            let vad = VoiceActivityDetector::create(&cfg, 60.0)
                .ok_or_else(|| SttError::Load("could not create the Silero VAD".into()))?;
            Ok(Self {
                vad,
                in_speech: false,
            })
        }

        fn drain(&mut self, out: &mut Vec<VadEvent>) {
            if self.vad.detected() && !self.in_speech {
                self.in_speech = true;
                out.push(VadEvent::Start);
            }
            while !self.vad.is_empty() {
                if let Some(seg) = self.vad.front() {
                    if !self.in_speech {
                        out.push(VadEvent::Start);
                    }
                    out.push(VadEvent::End(Segment {
                        start: seg.start().max(0) as u64,
                        samples: seg.samples().to_vec(),
                    }));
                }
                self.vad.pop();
                self.in_speech = false;
            }
        }
    }

    impl SpeechSegmenter for SileroSegmenter {
        fn push(&mut self, pcm: &[f32], out: &mut Vec<VadEvent>) {
            self.vad.accept_waveform(pcm);
            self.drain(out);
        }

        fn flush(&mut self, out: &mut Vec<VadEvent>) {
            self.vad.flush();
            self.drain(out);
        }
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    /// Deterministic pseudo-noise at `amp` (uniform, so RMS is about amp/sqrt(3)).
    pub fn noise(n: usize, amp: f32, seed: &mut u32) -> Vec<f32> {
        (0..n)
            .map(|_| {
                *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((*seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0 * amp
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::noise;
    use super::*;

    fn run(seg: &mut EnergySegmenter, pcm: &[f32]) -> Vec<VadEvent> {
        let mut out = Vec::new();
        for c in pcm.chunks(1600) {
            seg.push(c, &mut out);
        }
        seg.flush(&mut out);
        out
    }

    #[test]
    fn silence_produces_nothing() {
        let mut seed = 1;
        let mut s = EnergySegmenter::default();
        let quiet = noise(RATE * 20, 0.002, &mut seed);
        assert!(run(&mut s, &quiet).is_empty());
    }

    #[test]
    fn speech_burst_becomes_one_segment_with_preroll_and_hangover() {
        let mut seed = 7;
        let mut pcm = noise(RATE * 2, 0.002, &mut seed);
        let onset = pcm.len();
        pcm.extend(noise(RATE, 0.3, &mut seed)); // 1 s of "speech"
        pcm.extend(noise(RATE * 3, 0.002, &mut seed));
        let mut s = EnergySegmenter::default();
        let ev = run(&mut s, &pcm);
        assert_eq!(ev.len(), 2, "{ev:?}");
        assert_eq!(ev[0], VadEvent::Start);
        let VadEvent::End(seg) = &ev[1] else {
            panic!("no segment")
        };
        // pre-roll: starts before the onset by 300..350 ms
        let lead = onset as i64 - seg.start as i64;
        assert!((4800..=6000).contains(&lead), "lead {lead}");
        // 1 s speech + hangover (~0.6 s) + pre-roll (~0.3 s)
        assert!((1.7..2.3).contains(&seg.seconds()), "{}", seg.seconds());
    }

    #[test]
    fn short_clicks_are_dropped() {
        let mut seed = 3;
        let mut pcm = noise(RATE * 2, 0.002, &mut seed);
        pcm.extend(noise(FRAME * 4, 0.5, &mut seed)); // 128 ms click
        pcm.extend(noise(RATE * 3, 0.002, &mut seed));
        let mut s = EnergySegmenter::default();
        let ev = run(&mut s, &pcm);
        assert!(!ev.iter().any(|e| matches!(e, VadEvent::End(_))), "{ev:?}");
    }

    #[test]
    fn two_utterances_two_segments() {
        let mut seed = 11;
        let mut pcm = noise(RATE * 2, 0.002, &mut seed);
        for _ in 0..2 {
            pcm.extend(noise(RATE, 0.3, &mut seed));
            pcm.extend(noise(RATE * 2, 0.002, &mut seed));
        }
        let mut s = EnergySegmenter::default();
        let n = run(&mut s, &pcm)
            .iter()
            .filter(|e| matches!(e, VadEvent::End(_)))
            .count();
        assert_eq!(n, 2);
    }

    #[test]
    fn arbitrary_chunk_sizes_give_the_same_result() {
        let mut seed = 5;
        let mut pcm = noise(RATE * 2, 0.002, &mut seed);
        pcm.extend(noise(RATE, 0.3, &mut seed));
        pcm.extend(noise(RATE * 2, 0.002, &mut seed));
        let a = run(&mut EnergySegmenter::default(), &pcm);
        let mut s = EnergySegmenter::default();
        let mut b = Vec::new();
        for c in pcm.chunks(333) {
            s.push(c, &mut b);
        }
        s.flush(&mut b);
        assert_eq!(a, b);
    }

    #[test]
    fn long_speech_is_cut_at_the_limit() {
        let mut seed = 9;
        let mut pcm = noise(RATE, 0.002, &mut seed);
        pcm.extend(noise(RATE * 70, 0.3, &mut seed));
        let ev = run(&mut EnergySegmenter::default(), &pcm);
        let ends: Vec<&Segment> = ev
            .iter()
            .filter_map(|e| {
                if let VadEvent::End(s) = e {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();
        assert!(ends.len() >= 2);
        assert!(ends.iter().all(|s| s.seconds() <= 31.0));
    }

    /// PDA-T-0136 acceptance proxy: ten minutes of room noise cost far less than 3 % of one core and
    /// never produce a segment (so the STT is never invoked).
    #[test]
    fn ten_minutes_of_silence_is_cheap() {
        let mut seed = 21;
        let mut s = EnergySegmenter::default();
        let mut out = Vec::new();
        let block = noise(RATE, 0.003, &mut seed);
        let t = std::time::Instant::now();
        for _ in 0..600 {
            s.push(&block, &mut out);
        }
        let el = t.elapsed().as_secs_f64();
        assert!(out.is_empty());
        let pct = 100.0 * el / 600.0;
        println!("energy VAD: {pct:.4} % of one core");
        assert!(pct < 3.0, "{pct}");
    }
}
