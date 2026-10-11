//! Pure organism simulation: `step()` and `shape()` produce a `DrawList`.

/// Crate name, used by the skeleton smoke test.
pub const CRATE_NAME: &str = "organism";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_set() {
        assert_eq!(CRATE_NAME, "organism");
    }
}
