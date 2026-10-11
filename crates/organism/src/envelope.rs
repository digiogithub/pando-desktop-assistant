//! Voice amplitude: the smoothed `0..1` value that drives the ripple and nucleus size.

use crate::params::Params;
use crate::types::BodyState;

/// Smoothed amplitude plus the clock of the synthetic envelope.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Amplitude {
    /// Current smoothed amplitude.
    pub value: f64,
    /// Time accumulated while listening or speaking (reference `ampT`).
    pub tau: f64,
}

/// Synthetic target for a state and envelope clock (reference L60-64).
#[must_use]
pub fn synthetic_target(p: &Params, state: BodyState, tau: f64) -> f64 {
    match state {
        BodyState::Listening => 0.35 + 0.45 * ((tau * 6.3).sin() * (tau * 1.7).sin()).abs(),
        BodyState::Speaking => 0.3 + 0.5 * ((tau * 9.1).sin() * (tau * 2.3).cos()).abs(),
        BodyState::Thinking => p.envelope.thinking,
        BodyState::Idle | BodyState::Delegating => p.envelope.idle,
    }
}

impl Amplitude {
    /// Advance by `dt`. `rms` overrides the synthetic target while listening or speaking.
    pub fn update(&mut self, p: &Params, state: BodyState, rms: Option<f64>, dt: f64) {
        if matches!(state, BodyState::Listening | BodyState::Speaking) {
            self.tau += dt;
        }
        let target = match (state, rms) {
            (BodyState::Listening | BodyState::Speaking, Some(r)) => r.clamp(0.0, 1.0),
            _ => synthetic_target(p, state, self.tau),
        };
        self.value += (target - self.value) * (dt * p.physics.amplitude_lerp_per_second).min(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_to_idle_target() {
        let p = Params::default();
        let mut a = Amplitude {
            value: 1.0,
            tau: 0.0,
        };
        for _ in 0..120 {
            a.update(&p, BodyState::Idle, None, 1.0 / 60.0);
        }
        assert!((a.value - p.envelope.idle).abs() < 1e-3);
    }

    #[test]
    fn external_rms_wins_while_speaking() {
        let p = Params::default();
        let mut a = Amplitude::default();
        for _ in 0..120 {
            a.update(&p, BodyState::Speaking, Some(0.9), 1.0 / 60.0);
        }
        assert!((a.value - 0.9).abs() < 1e-3);
        // Not while thinking.
        for _ in 0..120 {
            a.update(&p, BodyState::Thinking, Some(0.9), 1.0 / 60.0);
        }
        assert!((a.value - p.envelope.thinking).abs() < 1e-3);
    }

    #[test]
    fn synthetic_listening_stays_in_range() {
        let p = Params::default();
        for i in 0..1000 {
            let v = synthetic_target(&p, BodyState::Listening, f64::from(i) * 0.013);
            assert!((0.35..=0.8).contains(&v));
        }
    }
}
