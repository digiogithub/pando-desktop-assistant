//! UI language selection (ADR-0003).
//!
//! Translations live in `assets/locales/<namespace>.<lang>.yml` and are loaded by `rust-i18n`
//! (the same instance GPUI Kit uses). Spanish is the source locale, English the second. The
//! setting `language` is an explicit tag from [`LANGUAGES`] or `None`, which follows the
//! operating system and falls back to Spanish.

/// Languages the UI ships, as `(tag, native name)`.
pub const LANGUAGES: &[(&str, &str)] = &[("es", "Espa\u{f1}ol"), ("en", "English")];

/// The source and fallback language.
pub const DEFAULT: &str = "es";

/// Reduces a locale such as `es_ES.UTF-8` or `en-US` to a supported tag.
#[must_use]
pub fn match_tag(raw: &str) -> Option<&'static str> {
    let primary = raw
        .split(['-', '_', '.', '@'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    LANGUAGES
        .iter()
        .map(|(tag, _)| *tag)
        .find(|tag| *tag == primary)
}

/// The tag to use for a stored `setting` (`None` = follow the system).
#[must_use]
pub fn resolve(setting: Option<&str>) -> &'static str {
    // Tests assert one locale; they must not depend on the developer's OS language.
    let system = || {
        if cfg!(test) {
            Some("en")
        } else {
            sys_locale::get_locale().as_deref().and_then(match_tag)
        }
    };
    setting
        .and_then(match_tag)
        .or_else(system)
        .unwrap_or(DEFAULT)
}

/// Makes `setting` the active language.
pub fn apply(setting: Option<&str>) {
    rust_i18n::set_locale(resolve(setting));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_i18n::t;

    #[test]
    fn tags_are_matched_loosely() {
        assert_eq!(match_tag("es_ES.UTF-8"), Some("es"));
        assert_eq!(match_tag("en-US"), Some("en"));
        assert_eq!(match_tag("FR"), None);
        assert_eq!(resolve(Some("es")), "es");
        assert_eq!(
            resolve(Some("xx")),
            "en",
            "tests force English for the system"
        );
        assert_eq!(resolve(None), "en");
    }

    #[test]
    fn both_locales_define_every_key() {
        for key in [
            "app.name",
            "window.title",
            "titlebar.project_none",
            "titlebar.status_pando",
            "titlebar.status_voice",
            "main.placeholder",
        ] {
            let es = t!(key, locale = "es");
            let en = t!(key, locale = "en");
            assert_ne!(es, key, "es is missing {key}");
            assert_ne!(en, key, "en is missing {key}");
        }
        assert_ne!(
            t!("main.placeholder", locale = "es"),
            t!("main.placeholder", locale = "en")
        );
    }
}
