//! Hold-to-talk state machine shared by every trigger (global hotkey, portal, CLI, tray).
//!
//! It is idempotent on purpose: X11 auto-repeat sends extra `Pressed` events and a compositor
//! binding may deliver `stop` without `start`; only real edges are reported.

use super::ipc::PttAction;

/// A raw input from a trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PttInput {
    /// Key down / `ptt start`.
    Pressed,
    /// Key up / `ptt stop`.
    Released,
    /// Flip.
    Toggle,
}

impl From<PttAction> for PttInput {
    fn from(a: PttAction) -> Self {
        match a {
            PttAction::Start => Self::Pressed,
            PttAction::Stop => Self::Released,
            PttAction::Toggle => Self::Toggle,
        }
    }
}

/// An actual change of the listening state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PttEdge {
    /// Start capturing.
    Started,
    /// Stop capturing and send.
    Stopped,
}

/// Tracks whether the user is holding the talk trigger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PttMachine {
    listening: bool,
}

impl PttMachine {
    /// Idle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether capture is on.
    #[must_use]
    pub fn listening(&self) -> bool {
        self.listening
    }

    /// Applies an input; `Some` only when the state changed.
    pub fn apply(&mut self, input: PttInput) -> Option<PttEdge> {
        let want = match input {
            PttInput::Pressed => true,
            PttInput::Released => false,
            PttInput::Toggle => !self.listening,
        };
        if want == self.listening {
            return None;
        }
        self.listening = want;
        Some(if want {
            PttEdge::Started
        } else {
            PttEdge::Stopped
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_cycle_and_idempotence() {
        let mut m = PttMachine::new();
        assert_eq!(m.apply(PttInput::Released), None);
        assert_eq!(m.apply(PttInput::Pressed), Some(PttEdge::Started));
        assert_eq!(m.apply(PttInput::Pressed), None, "auto-repeat");
        assert!(m.listening());
        assert_eq!(m.apply(PttInput::Released), Some(PttEdge::Stopped));
        assert_eq!(m.apply(PttInput::Released), None);
    }

    #[test]
    fn toggle_flips() {
        let mut m = PttMachine::new();
        assert_eq!(m.apply(PttInput::Toggle), Some(PttEdge::Started));
        assert_eq!(m.apply(PttInput::Toggle), Some(PttEdge::Stopped));
        assert_eq!(PttInput::from(PttAction::Start), PttInput::Pressed);
    }
}
