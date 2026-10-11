//! Claude Code hooks receiver and channel.

/// Crate name, used by the skeleton smoke test.
pub const CRATE_NAME: &str = "claude-bridge";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_set() {
        assert_eq!(CRATE_NAME, "claude-bridge");
    }
}
