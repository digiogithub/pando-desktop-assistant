//! Microphone and speaker levels for the organism (PDA-T-0085).
//!
//! `audio::Capture` already normalises the RMS of every 20 ms frame against a slow-tracking
//! peak ([`audio::LevelMeter`]). This module adds what the organism needs on top: a noise
//! gate with hysteresis (a quiet room reads 0, not the hiss normalised to a tiny peak) and
//! attack/release smoothing so the body breathes instead of flickering. It is polled from the
//! UI at the display rate, and is pure so the behaviour is unit-tested.

use std::time::Duration;

/// Below this the gate closes.
pub const GATE_CLOSE: f32 = 0.06;
/// Above this the gate opens.
pub const GATE_OPEN: f32 = 0.12;
/// Smoothing time constant while the level rises.
pub const ATTACK: Duration = Duration::from_millis(25);
/// Smoothing time constant while the level falls.
pub const RELEASE: Duration = Duration::from_millis(140);

/// One gated, smoothed level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelSmoother {
    open: bool,
    value: f32,
}

impl Default for LevelSmoother {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelSmoother {
    /// A closed gate at level 0.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            open: false,
            value: 0.0,
        }
    }

    /// Advances by `dt` with the normalised level `raw` (`0..=1`); returns the output level.
    pub fn step(&mut self, raw: f32, dt: Duration) -> f32 {
        let raw = if raw.is_finite() {
            raw.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if self.open {
            if raw < GATE_CLOSE {
                self.open = false;
            }
        } else if raw > GATE_OPEN {
            self.open = true;
        }
        let target = if self.open { raw } else { 0.0 };
        let tau = if target > self.value { ATTACK } else { RELEASE };
        let k = 1.0 - (-dt.as_secs_f32() / tau.as_secs_f32()).exp();
        self.value += (target - self.value) * k;
        if self.value < 0.002 {
            self.value = 0.0;
        }
        self.value
    }

    /// Current output.
    #[must_use]
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Resets to silence.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

/// Mic and TTS levels together, as handed to `OrganismView::set_levels`.
#[derive(Debug, Clone, Default)]
pub struct OrganismLevels {
    mic: LevelSmoother,
    tts: LevelSmoother,
    tts_meter: audio::LevelMeter,
}

impl OrganismLevels {
    /// `mic`: normalised capture level while listening (`None` = not listening, so the
    /// organism keeps its synthetic envelope); `tts_rms`: raw output RMS while speaking.
    /// Returns `(mic, tts)` for `set_levels`.
    pub fn tick(
        &mut self,
        mic: Option<f32>,
        tts_rms: Option<f32>,
        dt: Duration,
    ) -> (Option<f32>, Option<f32>) {
        let mic = match mic {
            Some(raw) => Some(self.mic.step(raw, dt)),
            None => {
                self.mic.reset();
                None
            }
        };
        let tts = match tts_rms {
            Some(rms) => {
                let norm = self.tts_meter.update(rms);
                Some(self.tts.step(norm, dt))
            }
            None => {
                self.tts.reset();
                None
            }
        };
        (mic, tts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(20);

    fn settle(s: &mut LevelSmoother, raw: f32, frames: usize) -> f32 {
        let mut v = 0.0;
        for _ in 0..frames {
            v = s.step(raw, FRAME);
        }
        v
    }

    #[test]
    fn silence_and_hiss_read_zero() {
        let mut s = LevelSmoother::new();
        assert_eq!(settle(&mut s, 0.0, 50), 0.0);
        assert_eq!(settle(&mut s, 0.08, 50), 0.0, "below the opening threshold");
    }

    #[test]
    fn soft_and_loud_speech_are_distinguishable() {
        let mut soft = LevelSmoother::new();
        let mut loud = LevelSmoother::new();
        let a = settle(&mut soft, 0.3, 25);
        let b = settle(&mut loud, 0.9, 25);
        assert!(b - a > 0.4, "{a} vs {b}");
        assert!(a > 0.2);
    }

    #[test]
    fn gate_has_hysteresis() {
        let mut s = LevelSmoother::new();
        settle(&mut s, 0.5, 20);
        // 0.08 is under the opening threshold but over the closing one: stays open.
        let v = settle(&mut s, 0.08, 60);
        assert!((v - 0.08).abs() < 0.01, "{v}");
        // Dropping under the closing threshold closes it.
        assert_eq!(settle(&mut s, 0.02, 100), 0.0);
    }

    #[test]
    fn release_is_slower_than_attack() {
        let mut s = LevelSmoother::new();
        let up = s.step(1.0, FRAME);
        let mut s2 = LevelSmoother::new();
        settle(&mut s2, 1.0, 60);
        let down = 1.0 - s2.step(0.0, FRAME);
        assert!(up > down, "{up} {down}");
    }

    #[test]
    fn garbage_input_is_harmless() {
        let mut s = LevelSmoother::new();
        assert_eq!(s.step(f32::NAN, FRAME), 0.0);
        assert!(s.step(f32::INFINITY, FRAME) <= 1.0);
        assert!(s.step(7.0, FRAME) <= 1.0);
    }

    #[test]
    fn not_listening_yields_none_so_the_organism_keeps_its_envelope() {
        let mut l = OrganismLevels::default();
        assert_eq!(l.tick(None, None, FRAME), (None, None));
        let (mic, tts) = l.tick(Some(0.9), Some(0.1), FRAME);
        assert!(mic.is_some_and(|m| m > 0.0));
        assert!(tts.is_some());
        assert_eq!(l.tick(None, None, FRAME), (None, None));
    }
}
