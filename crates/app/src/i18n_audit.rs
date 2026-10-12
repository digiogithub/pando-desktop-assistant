//! Locale audit (PDA-T-0127), compiled only for tests: every key used in code exists in both
//! languages, both languages define the same keys and placeholders, and no UI literal is
//! passed to a widget outside the locale files.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn locales_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/locales")
}

/// Flattens the two-space-indented YAML of a locale file into `key.path -> value`.
fn flatten(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim_start().is_empty() || trimmed.trim_start().starts_with('#') {
            continue;
        }
        let indent = trimmed.len() - trimmed.trim_start().len();
        let body = trimmed.trim_start();
        let Some((key, value)) = body.split_once(':') else {
            continue;
        };
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }
        let value = value.trim();
        let path = stack
            .iter()
            .map(|(_, k)| k.as_str())
            .chain(std::iter::once(key.trim()))
            .collect::<Vec<_>>()
            .join(".");
        if value.is_empty() {
            stack.push((indent, key.trim().to_owned()));
        } else {
            out.insert(path, value.trim_matches(['"', '\'']).to_owned());
        }
    }
    out
}

fn load(lang: &str) -> BTreeMap<String, String> {
    let mut all = BTreeMap::new();
    for entry in std::fs::read_dir(locales_dir())
        .expect("locales dir")
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(&format!(".{lang}.yml")) {
            let text = std::fs::read_to_string(entry.path()).expect("read locale");
            for (k, v) in flatten(&text) {
                assert!(
                    all.insert(k.clone(), v).is_none(),
                    "{lang}: duplicate key {k}"
                );
            }
        }
    }
    all
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("src dir").flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Source of a file without its `#[cfg(test)]` module (tests may use literals).
fn production_part(text: &str) -> &str {
    text.find("#[cfg(test)]").map_or(text, |i| &text[..i])
}

fn sources() -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    files
        .into_iter()
        .filter(|f| !f.ends_with("i18n_audit.rs"))
        .map(|f| {
            let t = std::fs::read_to_string(&f).expect("read source");
            (f, production_part(&t).to_owned())
        })
        .collect()
}

/// String literals that are the first argument of `t!(` / `tr(` / `super::tr(`.
fn used_keys(text: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for marker in ["t!(\"", "tr(\""] {
        let mut rest = text;
        while let Some(i) = rest.find(marker) {
            let after = &rest[i + marker.len()..];
            if let Some(end) = after.find('"') {
                let k = &after[..end];
                if k.contains('.') && !k.contains(' ') && !k.contains('{') {
                    keys.insert(k.to_owned());
                }
            }
            rest = &rest[i + marker.len()..];
        }
    }
    keys
}

fn placeholders(v: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = v;
    while let Some(i) = rest.find("%{") {
        if let Some(j) = rest[i..].find('}') {
            out.insert(rest[i + 2..i + j].to_owned());
        }
        rest = &rest[i + 2..];
    }
    out
}

#[test]
fn both_languages_define_the_same_keys_and_placeholders() {
    let (es, en) = (load("es"), load("en"));
    let only_es: Vec<_> = es.keys().filter(|k| !en.contains_key(*k)).collect();
    let only_en: Vec<_> = en.keys().filter(|k| !es.contains_key(*k)).collect();
    assert!(only_es.is_empty(), "keys missing in en: {only_es:?}");
    assert!(only_en.is_empty(), "keys missing in es: {only_en:?}");
    for (k, v) in &es {
        assert_eq!(
            placeholders(v),
            placeholders(&en[k]),
            "placeholders of {k} differ"
        );
        assert!(
            !v.trim().is_empty() && !en[k].trim().is_empty(),
            "{k} is empty"
        );
    }
}

#[test]
fn every_key_used_in_code_exists_in_both_languages() {
    let (es, en) = (load("es"), load("en"));
    let mut missing = Vec::new();
    for (file, text) in sources() {
        for k in used_keys(&text) {
            for (lang, map) in [("es", &es), ("en", &en)] {
                if !map.contains_key(&k) {
                    missing.push(format!("{lang}: {k} ({})", file.display()));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "untranslated keys:\n{}",
        missing.join("\n")
    );
}

#[test]
fn translated_text_is_really_translated() {
    // Spanish is the source: identical es/en values are fine for names and symbols only.
    let (es, en) = (load("es"), load("en"));
    let same: Vec<_> = es
        .iter()
        .filter(|(k, v)| {
            en[*k] == **v
                && v.chars().filter(|c| c.is_alphabetic()).count() > 12
                // Product names, URLs and pure format strings are the same in every language.
                && !(v.contains("%{") || v.contains("://") || v.contains("Pando Assistant") || v.contains("Claude Code"))
        })
        .map(|(k, _)| k.as_str())
        .collect();
    assert!(
        same.is_empty(),
        "long strings identical in es and en (untranslated?): {same:?}"
    );
}

/// Widget calls that take display text; their first argument must not be a literal with letters.
const DISPLAY_CALLS: &[&str] = &[
    ".label(\"",
    ".child(\"",
    ".tooltip(\"",
    ".placeholder(\"",
    ".title(\"",
];

#[test]
fn no_ui_literals_outside_locale_files() {
    let mut offenders = Vec::new();
    for (file, text) in sources() {
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or(line);
            for call in DISPLAY_CALLS {
                if let Some(i) = code.find(call) {
                    let lit = &code[i + call.len()..];
                    let lit = lit.split('"').next().unwrap_or("");
                    if lit.chars().any(char::is_alphabetic) {
                        offenders.push(format!("{}:{}: {}", file.display(), n + 1, code.trim()));
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "UI literals (move them to assets/locales):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn flatten_handles_nesting_and_quotes() {
    let m = flatten("a:\n  b: x\n  c:\n    d: \"y z\"\ne: w\n");
    assert_eq!(m["a.b"], "x");
    assert_eq!(m["a.c.d"], "y z");
    assert_eq!(m["e"], "w");
}
