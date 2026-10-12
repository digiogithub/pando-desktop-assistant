//! Hands-free barge-in policy while the assistant is speaking (`PDA-SP-0011.R4`).
//!
//! Pure state machine fed with one VAD decision per 20 ms frame of the echo-cleaned signal. During
//! TTS: speech must last >= 200 ms to duck the output by 12 dB, and 400 ms to stop it (speech in the
//! first 400 ms of a playback is ignored: the echo canceller is still converging). Without AEC and
//! without a headset hands-free barge-in is disabled (push-to-talk still works).

/// Frame duration the machine is driven with.
pub const FRAME_MS: u32 = 20;

/// What the caller must do with the playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BargeAction {
    None,
    /// Lower the output by 12 dB.
    Duck,
    /// The user kept talking: stop playback and start listening.
    Stop,
    /// It was a false alarm: restore the volume.
    Restore,
}

/// Barge-in detector.
#[derive(Debug)]
pub struct BargeIn {
    enabled: bool,
    speech_ms: u32,
    silence_ms: u32,
    ducked: bool,
    /// Milliseconds the current playback has been audible.
    playing_ms: u32,
}

impl BargeIn {
    /// `aec_or_headset`: an echo canceller is running or the user wears a headset.
    pub fn new(aec_or_headset: bool) -> Self {
        Self {
            enabled: aec_or_headset,
            speech_ms: 0,
            silence_ms: 0,
            ducked: false,
            playing_ms: 0,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The echo canceller has not converged on the first words of an utterance (measured on a
    /// real loopback: 200-250 ms of residual that the VAD takes for speech), so speech is not
    /// counted until the playback has been audible this long.
    pub const GRACE_MS: u32 = 400;

    /// Output gain to apply while ducked (-12 dB).
    pub const DUCK_GAIN: f32 = 0.251;

    /// One frame: `playing` = TTS is audible, `speech` = VAD on the cleaned microphone signal.
    pub fn frame(&mut self, playing: bool, speech: bool) -> BargeAction {
        if !self.enabled || !playing {
            let was = std::mem::take(&mut self.ducked);
            self.speech_ms = 0;
            self.silence_ms = 0;
            self.playing_ms = 0;
            return if was {
                BargeAction::Restore
            } else {
                BargeAction::None
            };
        }
        self.playing_ms = self.playing_ms.saturating_add(FRAME_MS);
        let speech = speech && self.playing_ms > Self::GRACE_MS;
        if speech {
            self.speech_ms += FRAME_MS;
            self.silence_ms = 0;
        } else {
            self.silence_ms += FRAME_MS;
            // short gaps inside a word do not reset the count; a real pause does
            if self.silence_ms >= 200 {
                self.speech_ms = 0;
            }
        }
        if self.speech_ms >= 400 {
            self.speech_ms = 0;
            self.ducked = false;
            return BargeAction::Stop;
        }
        if self.speech_ms >= 200 && !self.ducked {
            self.ducked = true;
            return BargeAction::Duck;
        }
        if self.ducked && self.silence_ms >= 200 {
            self.ducked = false;
            return BargeAction::Restore;
        }
        BargeAction::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(b: &mut BargeIn, playing: bool, pattern: &[(bool, u32)]) -> Vec<(u32, BargeAction)> {
        let mut t = 0;
        let mut out = Vec::new();
        for &(speech, ms) in pattern {
            for _ in 0..ms / FRAME_MS {
                t += FRAME_MS;
                let a = b.frame(playing, speech);
                if a != BargeAction::None {
                    out.push((t, a));
                }
            }
        }
        out
    }

    #[test]
    fn duck_at_200_ms_and_stop_at_400_ms() {
        let mut b = BargeIn::new(true);
        let ev = drive(&mut b, true, &[(false, BargeIn::GRACE_MS), (true, 500)]);
        assert_eq!(ev, vec![(600, BargeAction::Duck), (800, BargeAction::Stop)]);
    }

    #[test]
    fn echo_residual_while_the_canceller_converges_is_ignored() {
        let mut b = BargeIn::new(true);
        // 400 ms of "speech" right at the start of the playback, then clean signal.
        assert!(drive(&mut b, true, &[(true, BargeIn::GRACE_MS), (false, 600)]).is_empty());
        // The grace period restarts with the next playback.
        drive(&mut b, false, &[(false, 100)]);
        assert!(drive(&mut b, true, &[(true, BargeIn::GRACE_MS), (false, 600)]).is_empty());
    }

    #[test]
    fn a_short_noise_does_not_duck() {
        let mut b = BargeIn::new(true);
        assert!(
            drive(
                &mut b,
                true,
                &[(false, BargeIn::GRACE_MS), (true, 160), (false, 1000)]
            )
            .is_empty()
        );
    }

    #[test]
    fn duck_is_restored_after_a_pause() {
        let mut b = BargeIn::new(true);
        let ev = drive(
            &mut b,
            true,
            &[(false, BargeIn::GRACE_MS), (true, 240), (false, 400)],
        );
        assert_eq!(ev[0], (600, BargeAction::Duck));
        assert!(ev.iter().any(|(_, a)| *a == BargeAction::Restore));
    }

    #[test]
    fn disabled_without_aec_or_headset() {
        let mut b = BargeIn::new(false);
        assert!(!b.is_enabled());
        assert!(drive(&mut b, true, &[(true, 2000)]).is_empty());
    }

    #[test]
    fn nothing_when_not_playing() {
        let mut b = BargeIn::new(true);
        assert!(drive(&mut b, false, &[(true, 2000)]).is_empty());
    }

    #[test]
    fn playback_ending_while_ducked_restores() {
        let mut b = BargeIn::new(true);
        drive(&mut b, true, &[(false, BargeIn::GRACE_MS), (true, 240)]);
        assert_eq!(b.frame(false, false), BargeAction::Restore);
    }
}
