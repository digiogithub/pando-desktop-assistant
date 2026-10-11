//! Pure organism simulation: `step()` and `shape()` produce a `DrawList`.

pub mod envelope;
pub mod noise;
pub mod params;
pub mod rng;
pub mod sim;
pub mod spring;
pub mod tokens;
pub mod types;

pub use params::{Extras, Params, W_REF};
pub use sim::{Config, Organism};
pub use types::{BodyState, CellId, CellInput, Engine, Inputs, Status};

/// Crate name, used by the skeleton smoke test.
pub const CRATE_NAME: &str = "organism";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_match_design_system() {
        use tokens::*;
        assert_eq!(SPRING_EASE, [0.34, 1.9, 0.5, 1.0]);
        assert_eq!(ENGINES.len(), 4);
        assert_eq!(ENGINES[1].nucleus, CLAUDE_BLUE);
        assert!(!ENGINES[3].can_talk && ENGINES[3].membrane == Membrane::Dashed);
        assert_eq!(STATUSES.iter().filter(|s| s.badge.is_some()).count(), 4);
        assert_eq!(ORGANISM_RING_RADIUS, 0.36);
        assert_eq!(MARFIL_ALPHA_62.a, 158);
    }

    #[test]
    fn crate_name_is_set() {
        assert_eq!(CRATE_NAME, "organism");
    }
}
