//! Text normalization shared by the dictation trigger, the literal-id rule of the
//! policy and the project pre-filter.

/// Fold one char: lowercase and strip the diacritics that matter for Spanish (and the
/// common Latin-1 ones). `ñ` folds to `n` because ids are ASCII (`migracion-auth`).
fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ã' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ñ' | 'Ñ' => 'n',
        'ç' | 'Ç' => 'c',
        c => c.to_lowercase().next().unwrap_or(c),
    }
}

/// Lowercase, fold accents, turn every non-alphanumeric run (dashes, underscores,
/// punctuation, spaces) into one space, and trim.
///
/// `"¿Cancelo Migración-Auth?"` → `"cancelo migracion auth"`.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for c in s.chars().map(fold) {
        if c.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(c);
        } else {
            pending_space = true;
        }
    }
    out
}

/// Whether `needle` appears in `haystack` as a whole-word sequence, after
/// [`normalize`] on both. An empty needle never matches.
pub fn contains_words(haystack: &str, needle: &str) -> bool {
    let n = normalize(needle);
    if n.is_empty() {
        return false;
    }
    let h = normalize(haystack);
    let (h, n) = (format!(" {h} "), format!(" {n} "));
    h.contains(&n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn normalizes_spanish_and_separators() {
        assert_eq!(
            normalize("¿Cancelo Migración-Auth?"),
            "cancelo migracion auth"
        );
        assert_eq!(normalize("  bitácora_ui  "), "bitacora ui");
        assert_eq!(normalize("AÑO"), "ano");
        assert_eq!(normalize("--"), "");
    }

    #[test]
    fn whole_word_containment() {
        assert!(contains_words("cancela migración auth", "migracion-auth"));
        assert!(contains_words(
            "Cancela MIGRACION_AUTH ya",
            "migracion-auth"
        ));
        assert!(!contains_words("cancela migración", "migracion-auth"));
        assert!(!contains_words("cancela migracion-authz", "migracion-auth"));
        assert!(!contains_words("cancela la api", "ap"));
        assert!(!contains_words("anything", ""));
        assert!(!contains_words("anything", "--"));
    }

    proptest! {
        #[test]
        fn normalize_is_idempotent(s in "\\PC{0,80}") {
            let once = normalize(&s);
            prop_assert_eq!(normalize(&once), once.clone());
            prop_assert!(!once.starts_with(' ') && !once.ends_with(' '));
            prop_assert!(!once.contains("  "));
        }
    }
}
