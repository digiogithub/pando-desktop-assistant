//! Engine routing policy (ADR 0008): Whistle by default, Parakeet for long audio, "high
//! accuracy", Intel Macs, or when Whistle failed to load.

use crate::whistle::MAX_CHUNK_SECONDS;

/// Which engine handles a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Whistle,
    Parakeet,
}

/// Inputs of the routing decision.
#[derive(Debug, Clone, Copy, Default)]
pub struct Policy {
    pub parakeet_installed: bool,
    pub whistle_available: bool,
    /// User picked "high accuracy" in settings.
    pub high_accuracy: bool,
    /// Platform without a libneedle build (macOS x86_64).
    pub platform_lacks_whistle: bool,
}

impl Policy {
    /// Whether Whistle can be used at all.
    pub fn whistle_usable(&self) -> bool {
        self.whistle_available && !self.platform_lacks_whistle
    }
}

/// Pick the engine for a clip of `seconds`. `None` when no engine is usable.
pub fn select(policy: &Policy, seconds: f32) -> Option<Engine> {
    match (policy.whistle_usable(), policy.parakeet_installed) {
        (false, false) => None,
        (false, true) => Some(Engine::Parakeet),
        (true, false) => Some(Engine::Whistle),
        (true, true) => {
            if policy.high_accuracy || seconds > MAX_CHUNK_SECONDS {
                Some(Engine::Parakeet)
            } else {
                Some(Engine::Whistle)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(w: bool, k: bool) -> Policy {
        Policy {
            whistle_available: w,
            parakeet_installed: k,
            ..Default::default()
        }
    }

    #[test]
    fn whistle_is_the_default() {
        assert_eq!(select(&p(true, false), 20.0), Some(Engine::Whistle));
        assert_eq!(select(&p(true, true), 3.0), Some(Engine::Whistle));
    }

    #[test]
    fn long_audio_or_high_accuracy_uses_parakeet_when_installed() {
        assert_eq!(select(&p(true, true), 8.0), Some(Engine::Parakeet));
        let mut pol = p(true, true);
        pol.high_accuracy = true;
        assert_eq!(select(&pol, 1.0), Some(Engine::Parakeet));
    }

    #[test]
    fn falls_back_when_whistle_is_missing() {
        assert_eq!(select(&p(false, true), 1.0), Some(Engine::Parakeet));
        let mut pol = p(true, true);
        pol.platform_lacks_whistle = true;
        assert_eq!(select(&pol, 1.0), Some(Engine::Parakeet));
        assert_eq!(select(&p(false, false), 1.0), None);
    }
}
