//! A tiny energy VAD for the hands-free barge-in policy (PDA-T-0140): one decision per 20 ms
//! frame of the echo-cleaned microphone signal. The accurate VAD and the wake verification live
//! in the STT worker; this one only has to notice that someone is talking over the assistant,
//! and it adapts to the noise floor of the room.

/// Speech needs this much energy over the floor.
const RATIO: f32 = 3.0;
/// Never below this absolute RMS (about -46 dBFS).
const MIN_RMS: f32 = 0.005;

/// Adaptive energy VAD.
#[derive(Debug, Clone)]
pub struct EnergyVad {
    floor: f32,
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self::new()
    }
}

impl EnergyVad {
    /// A VAD that assumes a quiet room until it learns otherwise.
    #[must_use]
    pub fn new() -> Self {
        Self { floor: 0.002 }
    }

    /// Classifies one frame.
    pub fn frame(&mut self, frame: &[f32]) -> bool {
        let rms = audio::rms(frame);
        let speech = rms > (self.floor * RATIO).max(MIN_RMS);
        if !speech {
            // Learn the room: slow when rising, faster when falling.
            let k = if rms > self.floor { 0.01 } else { 0.1 };
            self.floor += (rms - self.floor) * k;
        } else {
            // Sustained noise that looks like speech slowly becomes the new floor.
            self.floor += (rms - self.floor) * 0.002;
        }
        speech
    }

    /// The learned noise floor.
    #[must_use]
    pub fn floor(&self) -> f32 {
        self.floor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amp: f32) -> Vec<f32> {
        (0..320).map(|i| amp * (i as f32 * 0.3).sin()).collect()
    }

    #[test]
    fn silence_is_not_speech_and_a_loud_burst_is() {
        let mut v = EnergyVad::new();
        for _ in 0..50 {
            assert!(!v.frame(&tone(0.001)));
        }
        assert!(v.frame(&tone(0.2)));
        assert!(!v.frame(&tone(0.001)));
    }

    #[test]
    fn it_adapts_to_a_noisy_room() {
        let mut v = EnergyVad::new();
        // Constant hiss at 0.01 RMS-ish is speech at first (3x the quiet floor), then the floor rises.
        let hiss = tone(0.012);
        let first = v.frame(&hiss);
        for _ in 0..400 {
            v.frame(&hiss);
        }
        assert!(first);
        assert!(
            !v.frame(&hiss),
            "the room's own noise stops counting as speech"
        );
        assert!(v.frame(&tone(0.3)));
    }

    #[test]
    fn empty_frames_are_harmless() {
        assert!(!EnergyVad::new().frame(&[]));
    }
}
