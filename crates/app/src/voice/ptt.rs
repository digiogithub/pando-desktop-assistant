//! Push-to-talk state machine (PDA-T-0094, `PDA-SP-0006.R3`).
//!
//! Pure: the caller feeds presses, releases and 16 kHz capture frames together with a
//! monotonic millisecond clock and gets back what to do. Taps shorter than 250 ms are ignored,
//! a recording is closed after 120 s, 300 ms of pre-roll are prepended, and a press while the
//! assistant speaks reports a barge-in.

/// Presses shorter than this are not transcribed.
pub const MIN_HOLD_MS: u64 = 250;
/// A recording is closed automatically after this long.
pub const MAX_HOLD_MS: u64 = 120_000;
/// Pre-roll prepended to the utterance (300 ms at 16 kHz).
pub const PREROLL_SAMPLES: usize = 4_800;

/// What a press did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pressed {
    /// Recording started (false when one was already running).
    pub started: bool,
    /// The assistant was speaking: stop playback first.
    pub barge_in: bool,
}

/// How a recording ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Finished {
    /// Nothing was recording.
    NotRecording,
    /// Shorter than [`MIN_HOLD_MS`]: nothing to transcribe, the body returns to Idle.
    Tap,
    /// A normal utterance (pre-roll included).
    Utterance {
        /// 16 kHz mono samples.
        pcm: Vec<f32>,
        /// Held time in milliseconds.
        held_ms: u64,
    },
    /// The 120 s limit was reached: transcribe what exists and tell the user.
    TooLong {
        /// 16 kHz mono samples.
        pcm: Vec<f32>,
    },
}

/// The push-to-talk machine.
#[derive(Debug, Default)]
pub struct Ptt {
    started_at: Option<u64>,
    pcm: Vec<f32>,
}

impl Ptt {
    /// A fresh, idle machine.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True while recording.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.started_at.is_some()
    }

    /// Key or button down. `preroll` is the capture ring snapshot (oldest first).
    pub fn press(&mut self, now_ms: u64, preroll: &[f32], tts_active: bool) -> Pressed {
        if self.is_recording() {
            // Key auto-repeat while held.
            return Pressed {
                started: false,
                barge_in: false,
            };
        }
        let take = preroll.len().min(PREROLL_SAMPLES);
        self.pcm.clear();
        self.pcm.extend_from_slice(&preroll[preroll.len() - take..]);
        self.started_at = Some(now_ms);
        Pressed {
            started: true,
            barge_in: tts_active,
        }
    }

    /// One capture frame. Returns `Some(TooLong)` when the limit was hit.
    pub fn frame(&mut self, frame: &[f32], now_ms: u64) -> Option<Finished> {
        let start = self.started_at?;
        if now_ms.saturating_sub(start) >= MAX_HOLD_MS {
            return Some(self.finish_too_long());
        }
        self.pcm.extend_from_slice(frame);
        None
    }

    /// Key or button up.
    pub fn release(&mut self, now_ms: u64) -> Finished {
        let Some(start) = self.started_at.take() else {
            return Finished::NotRecording;
        };
        let held_ms = now_ms.saturating_sub(start);
        let pcm = std::mem::take(&mut self.pcm);
        if held_ms < MIN_HOLD_MS {
            Finished::Tap
        } else if held_ms >= MAX_HOLD_MS {
            Finished::TooLong { pcm }
        } else {
            Finished::Utterance { pcm, held_ms }
        }
    }

    /// Samples recorded so far (pre-roll included).
    #[must_use]
    pub fn len(&self) -> usize {
        self.pcm.len()
    }

    /// True when nothing is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pcm.is_empty()
    }

    /// The samples after index `from`, for streaming them to the STT worker.
    #[must_use]
    pub fn since(&self, from: usize) -> &[f32] {
        self.pcm.get(from..).unwrap_or(&[])
    }

    /// Drops the recording (window lost focus, worker died).
    pub fn cancel(&mut self) {
        self.started_at = None;
        self.pcm.clear();
    }

    fn finish_too_long(&mut self) -> Finished {
        self.started_at = None;
        Finished::TooLong {
            pcm: std::mem::take(&mut self.pcm),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_tap_transcribes_nothing() {
        let mut p = Ptt::new();
        p.press(1_000, &[], false);
        p.frame(&[0.1; 320], 1_020);
        assert_eq!(p.release(1_200), Finished::Tap);
        assert!(!p.is_recording());
    }

    #[test]
    fn hold_returns_preroll_then_frames() {
        let mut p = Ptt::new();
        let pre = vec![0.5_f32; 10_000];
        let pressed = p.press(0, &pre, false);
        assert!(pressed.started && !pressed.barge_in);
        for i in 0..20 {
            assert!(p.frame(&[0.2; 320], 20 * (i + 1)).is_none());
        }
        match p.release(500) {
            Finished::Utterance { pcm, held_ms } => {
                assert_eq!(held_ms, 500);
                assert_eq!(
                    pcm.len(),
                    PREROLL_SAMPLES + 20 * 320,
                    "only 300 ms of pre-roll"
                );
                assert_eq!(pcm[0], 0.5);
                assert_eq!(*pcm.last().expect("samples"), 0.2);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn press_during_tts_is_a_barge_in() {
        let mut p = Ptt::new();
        assert!(p.press(0, &[], true).barge_in);
    }

    #[test]
    fn repeat_press_while_held_changes_nothing() {
        let mut p = Ptt::new();
        p.press(0, &[], false);
        let again = p.press(30, &[], true);
        assert!(!again.started && !again.barge_in);
    }

    #[test]
    fn the_limit_closes_the_recording() {
        let mut p = Ptt::new();
        p.press(0, &[], false);
        assert!(p.frame(&[0.1; 320], 60_000).is_none());
        let done = p.frame(&[0.1; 320], MAX_HOLD_MS).expect("closed");
        assert!(matches!(done, Finished::TooLong { ref pcm } if pcm.len() == 320));
        assert!(!p.is_recording());
        assert_eq!(p.release(MAX_HOLD_MS + 5), Finished::NotRecording);
    }

    #[test]
    fn release_without_press_and_cancel() {
        let mut p = Ptt::new();
        assert_eq!(p.release(5), Finished::NotRecording);
        p.press(0, &[0.1; 100], false);
        p.cancel();
        assert_eq!(p.release(1_000), Finished::NotRecording);
    }
}
