//! Decision layer: typed questions and policy.

/// Crate name, used by the skeleton smoke test.
pub const CRATE_NAME: &str = "decide";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_set() {
        assert_eq!(CRATE_NAME, "decide");
    }
}
