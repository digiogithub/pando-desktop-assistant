//! App to voice-worker protocol.

/// Crate name, used by the skeleton smoke test.
pub const CRATE_NAME: &str = "voice-proto";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_set() {
        assert_eq!(CRATE_NAME, "voice-proto");
    }
}
