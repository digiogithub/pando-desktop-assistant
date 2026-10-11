//! The body state machine: the base state comes from the voice pipeline (the app is the
//! authority), `Delegating` is a 1.5 s overlay that returns to the previous state.

use crate::types::BodyState;

/// Is `from -> to` listed in `docs/06-ui-mapping.md`? Staying in a state is always fine and
/// any state may enter the Delegating overlay (`Organism::delegate`).
#[must_use]
pub fn is_listed_transition(from: BodyState, to: BodyState) -> bool {
    use BodyState::{Delegating, Idle, Listening, Speaking, Thinking};
    from == to
        || to == Delegating
        || from == Delegating
        || matches!(
            (from, to),
            (Idle, Listening)
                | (Listening, Thinking | Idle)
                | (Thinking, Speaking | Idle)
                | (Speaking, Idle | Listening)
        )
}

/// Base state plus the Delegating overlay.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyMachine {
    base: BodyState,
    overlay_since: Option<f64>,
    /// Transitions the app made that are not in the documented table (diagnostics).
    unlisted: u32,
}

impl BodyMachine {
    /// Record the base state the app reports. Returns `true` if it changed.
    pub fn set_base(&mut self, state: BodyState) -> bool {
        if state == self.base {
            return false;
        }
        if !is_listed_transition(self.base, state) {
            self.unlisted += 1;
        }
        self.base = state;
        true
    }

    #[must_use]
    pub fn base(&self) -> BodyState {
        self.base
    }

    /// Number of reported transitions that are not in the documented table.
    #[must_use]
    pub fn unlisted_transitions(&self) -> u32 {
        self.unlisted
    }

    /// Start (or restart) the Delegating overlay at time `t`.
    pub fn delegate(&mut self, t: f64) {
        self.overlay_since = Some(t);
    }

    /// Expire the overlay once `duration` has elapsed.
    pub fn update(&mut self, t: f64, duration: f64) {
        if self.overlay_since.is_some_and(|s| t - s >= duration) {
            self.overlay_since = None;
        }
    }

    #[must_use]
    pub fn overlay_active(&self) -> bool {
        self.overlay_since.is_some()
    }

    /// Overlay progress in `0..1`, `None` when inactive.
    #[must_use]
    pub fn overlay_progress(&self, t: f64, duration: f64) -> Option<f64> {
        self.overlay_since
            .map(|s| ((t - s) / duration).clamp(0.0, 1.0))
    }

    /// The state to show: the base state, or `Delegating` while the overlay runs.
    #[must_use]
    pub fn shown(&self) -> BodyState {
        if self.overlay_since.is_some() {
            BodyState::Delegating
        } else {
            self.base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use BodyState::{Delegating, Idle, Listening, Speaking, Thinking};

    #[test]
    fn documented_transitions_are_listed() {
        for (a, b) in [
            (Idle, Listening),
            (Listening, Thinking),
            (Listening, Idle),
            (Thinking, Speaking),
            (Thinking, Idle),
            (Speaking, Idle),
            (Speaking, Listening),
        ] {
            assert!(is_listed_transition(a, b), "{a:?} -> {b:?}");
        }
        for s in BodyState::ALL {
            assert!(is_listed_transition(s, Delegating), "any -> Delegating");
        }
    }

    #[test]
    fn unlisted_transitions_are_counted_but_followed() {
        let mut m = BodyMachine::default();
        assert!(m.set_base(Speaking));
        assert_eq!(m.unlisted_transitions(), 1); // Idle -> Speaking is not in the table
        assert_eq!(m.base(), Speaking);
        assert!(!m.set_base(Speaking));
    }

    #[test]
    fn delegating_overlay_returns_to_the_previous_state() {
        let mut m = BodyMachine::default();
        m.set_base(Listening);
        m.set_base(Thinking);
        m.delegate(10.0);
        assert_eq!(m.shown(), Delegating);
        assert_eq!(m.base(), Thinking);
        m.update(11.0, 1.5);
        assert!(m.overlay_active());
        assert!((m.overlay_progress(11.0, 1.5).unwrap() - 2.0 / 3.0).abs() < 1e-12);
        m.update(11.5, 1.5);
        assert_eq!(m.shown(), Thinking);
        assert_eq!(m.overlay_progress(11.5, 1.5), None);
    }
}
