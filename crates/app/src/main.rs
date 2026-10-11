//! Entry point of `pando-assistant` (skeleton).

fn main() {
    // gpui-kit is linked but not used yet; the UI lands in later tasks.
    println!("pando-assistant {}", env!("CARGO_PKG_VERSION"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }
}
