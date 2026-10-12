//! Signal level helpers shared by capture (mic RMS -> organism) and playback (TTS RMS).

/// Root-mean-square of a block (0 for an empty block).
pub fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}

/// Normalises RMS values to `0..=1` against a slow-tracking peak, so a quiet microphone still
/// drives the organism and a loud one does not saturate it.
#[derive(Debug, Clone)]
pub struct LevelMeter {
    peak: f32,
    floor: f32,
    decay: f32,
}

impl LevelMeter {
    /// `floor`: smallest peak considered (about -40 dBFS = 0.01); `decay`: per-frame peak
    /// decay factor (0.995 at 50 frames/s is a half-life of about 2.8 s).
    pub fn new(floor: f32, decay: f32) -> Self {
        Self {
            peak: floor,
            floor,
            decay,
        }
    }

    /// Feed the RMS of one frame; returns the normalised level.
    pub fn update(&mut self, rms: f32) -> f32 {
        self.peak = (self.peak * self.decay).max(rms).max(self.floor);
        (rms / self.peak).clamp(0.0, 1.0)
    }
}

impl Default for LevelMeter {
    fn default() -> Self {
        Self::new(0.01, 0.995)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_of_known_signals() {
        assert_eq!(rms(&[]), 0.0);
        assert!((rms(&[0.5; 100]) - 0.5).abs() < 1e-6);
        let sine: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.1).sin()).collect();
        assert!((rms(&sine) - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01);
    }

    #[test]
    fn meter_normalises_to_the_peak() {
        let mut m = LevelMeter::default();
        assert!(
            (m.update(0.2) - 1.0).abs() < 1e-6,
            "first loud frame is the peak"
        );
        let half = m.update(0.1);
        assert!((half - 0.5).abs() < 0.02, "{half}");
        assert_eq!(m.update(0.0), 0.0);
    }

    #[test]
    fn peak_decays_so_quiet_input_recovers() {
        let mut m = LevelMeter::default();
        m.update(0.5);
        let mut last = 0.0;
        for _ in 0..1000 {
            last = m.update(0.02);
        }
        assert!(last > 0.9, "{last}");
    }

    #[test]
    fn silence_stays_zero_and_never_nan() {
        let mut m = LevelMeter::default();
        for _ in 0..10 {
            assert_eq!(m.update(0.0), 0.0);
        }
    }
}
