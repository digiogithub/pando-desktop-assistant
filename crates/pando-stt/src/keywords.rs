//! Keyword biasing list and language forcing (PDA-T-0092, ADR 0008).
//!
//! Whistle takes newline-separated words/phrases as a bias. [`KeywordList`] builds it from the
//! fixed assistant terms, project names, agent/cell ids and user terms, adds humanized variants
//! of ids (`migracion-auth` also as "migracion auth"), removes duplicates and keeps the list
//! within a size budget, dropping the lowest-priority sources first.

/// Terms always present.
pub const FIXED_TERMS: [&str; 5] = ["Pando", "Oye Pando", "mesnada", "Mesnada", "Bitácora"];

/// Maximum phrases handed to the engine.
pub const MAX_PHRASES: usize = 200;
/// Maximum total bytes of the newline-joined string.
pub const MAX_BYTES: usize = 4096;
/// Longest single phrase kept.
const MAX_PHRASE_CHARS: usize = 64;

/// Turn an identifier into the words a person would say: separators become spaces and camelCase
/// is split. `migracion-auth` -> `migracion auth`, `handlers_chat.go` -> `handlers chat go`.
pub fn humanize(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    let mut prev_lower = false;
    for c in id.chars() {
        if matches!(c, '-' | '_' | '.' | '/' | ':') {
            out.push(' ');
            prev_lower = false;
        } else if c.is_uppercase() && prev_lower {
            out.push(' ');
            out.extend(c.to_lowercase());
            prev_lower = false;
        } else {
            out.push(c);
            prev_lower = c.is_lowercase() || c.is_ascii_digit();
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Builder with priorities: fixed terms, then projects, then cells/agents, then user terms.
#[derive(Debug, Clone, Default)]
pub struct KeywordList {
    projects: Vec<String>,
    cells: Vec<String>,
    user: Vec<String>,
}

impl KeywordList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Project names or ids (from `/projects`).
    pub fn projects<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.projects.extend(it.into_iter().map(Into::into));
        self
    }

    /// Cell / agent labels and ids.
    pub fn cells<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.cells.extend(it.into_iter().map(Into::into));
        self
    }

    /// Free-form terms from settings.
    pub fn user_terms<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.user.extend(it.into_iter().map(Into::into));
        self
    }

    /// Final phrase list: deduplicated (case-insensitively), humanized variants right after the
    /// raw id, capped at [`MAX_PHRASES`] / [`MAX_BYTES`].
    pub fn build(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<String> = Vec::new();
        let mut bytes = 0usize;
        let sources = FIXED_TERMS
            .iter()
            .map(|s| s.to_string())
            .chain(self.projects.iter().cloned())
            .chain(self.cells.iter().cloned())
            .chain(self.user.iter().cloned());
        'outer: for raw in sources {
            let raw = raw.trim().to_string();
            let human = humanize(&raw);
            for cand in [raw, human] {
                if cand.is_empty() || cand.chars().count() > MAX_PHRASE_CHARS || cand.contains('\n')
                {
                    continue;
                }
                if !seen.insert(cand.to_lowercase()) {
                    continue;
                }
                if out.len() >= MAX_PHRASES || bytes + cand.len() + 1 > MAX_BYTES {
                    break 'outer;
                }
                bytes += cand.len() + 1;
                out.push(cand);
            }
        }
        out
    }
}

/// Newline-separated keyword string for the engine, `None` when empty.
pub fn to_engine_string(keywords: &[String]) -> Option<String> {
    let k: Vec<&str> = keywords
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.contains('\n'))
        .collect();
    if k.is_empty() {
        None
    } else {
        Some(k.join("\n"))
    }
}

/// Languages Whistle can be forced to.
pub const WHISTLE_LANGS: [&str; 7] = ["en", "de", "fr", "es", "it", "nl", "pl"];

/// Resolve a settings value or OS locale to the engine language code.
///
/// `""`/`"auto"` -> `None` (detect). `"es-ES"`, `"es_MX.UTF-8"`, `"ES"` -> `Some("es")`.
/// A language Whistle does not support also gives `None` (auto-detect) instead of an error.
pub fn resolve_language(setting: &str) -> Option<String> {
    let s = setting.trim().to_lowercase();
    if s.is_empty() || s == "auto" {
        return None;
    }
    let code: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    WHISTLE_LANGS.contains(&code.as_str()).then_some(code)
}

/// Default forced language when the settings say nothing (Spanish-first product).
pub const DEFAULT_LANGUAGE: &str = "es";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanizes_ids() {
        assert_eq!(humanize("migracion-auth"), "migracion auth");
        assert_eq!(humanize("handlers_chat.go"), "handlers chat go");
        assert_eq!(humanize("pandoClient"), "pando client");
        assert_eq!(humanize("  x  "), "x");
    }

    #[test]
    fn builds_priority_ordered_dedup_list() {
        let l = KeywordList::new()
            .projects(["pando-assistant", "Pando"])
            .cells(["migracion-auth"])
            .user_terms(["gintrack", "PANDO"])
            .build();
        assert_eq!(&l[..3], ["Pando", "Oye Pando", "mesnada"]);
        assert!(l.contains(&"pando-assistant".to_string()));
        assert!(l.contains(&"pando assistant".to_string()));
        assert!(l.contains(&"migracion auth".to_string()));
        assert_eq!(
            l.iter().filter(|k| k.eq_ignore_ascii_case("pando")).count(),
            1
        );
    }

    #[test]
    fn respects_the_budget() {
        let many: Vec<String> = (0..1000).map(|i| format!("proyecto-{i}")).collect();
        let l = KeywordList::new().projects(many).build();
        assert!(l.len() <= MAX_PHRASES);
        assert!(l.iter().map(|s| s.len() + 1).sum::<usize>() <= MAX_BYTES);
        assert!(l.contains(&"Pando".to_string()), "fixed terms survive");
    }

    #[test]
    fn drops_overlong_and_multiline() {
        let l = KeywordList::new()
            .user_terms(["a\nb".to_string(), "x".repeat(100)])
            .build();
        assert!(!l.iter().any(|k| k.contains('\n') || k.len() > 64));
    }

    #[test]
    fn engine_string() {
        assert_eq!(to_engine_string(&[]), None);
        assert_eq!(
            to_engine_string(&["a".into(), " ".into(), "b c".into()]).as_deref(),
            Some("a\nb c")
        );
    }

    #[test]
    fn language_forcing() {
        assert_eq!(resolve_language("es").as_deref(), Some("es"));
        assert_eq!(resolve_language("es-ES").as_deref(), Some("es"));
        assert_eq!(resolve_language("EN_us.UTF-8").as_deref(), Some("en"));
        assert_eq!(resolve_language("auto"), None);
        assert_eq!(resolve_language(""), None);
        assert_eq!(resolve_language("ja-JP"), None);
    }
}
