//! Keyword biasing list (placeholder; extended by PDA-T-0092).

/// Newline-separated keyword string for the engine, `None` when empty.
pub fn to_engine_string(keywords: &[String]) -> Option<String> {
    let k: Vec<&str> = keywords
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if k.is_empty() {
        None
    } else {
        Some(k.join("\n"))
    }
}
