//! The model manifest.

use serde::Deserialize;

use crate::ModelError;

/// The manifest that ships inside the binary.
pub const EMBEDDED: &str = include_str!("../models.toml");

/// One downloadable model.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    /// `stt`, `tts`, `vad`, `wake`.
    pub kind: String,
    pub title: String,
    pub url: String,
    /// Single-file models: the file name inside the model directory.
    #[serde(default)]
    pub file: Option<String>,
    /// Archive models: `tar.bz2`; extracted into the model directory with the top folder stripped.
    #[serde(default)]
    pub archive: Option<String>,
    /// SHA-256 of the downloaded file (the archive for archive models), lowercase hex.
    pub sha256: String,
    /// Exact size in bytes of the downloaded file.
    pub size: u64,
    pub license: String,
    #[serde(default)]
    pub attribution: String,
    /// Ships with the installer; the downloader only repairs it.
    #[serde(default)]
    pub bundled: bool,
    /// Files that must exist after extraction (archive models).
    #[serde(default)]
    pub required_files: Vec<String>,
}

impl ModelEntry {
    /// True when the licence needs an attribution line in NOTICE / About.
    pub fn needs_attribution(&self) -> bool {
        !self.attribution.is_empty()
    }
}

/// All models.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    #[serde(rename = "model")]
    pub models: Vec<ModelEntry>,
}

impl Manifest {
    /// The embedded manifest.
    pub fn embedded() -> Result<Self, ModelError> {
        Self::parse(EMBEDDED)
    }

    pub fn parse(text: &str) -> Result<Self, ModelError> {
        let m: Manifest = toml::from_str(text).map_err(|e| ModelError::Manifest(e.to_string()))?;
        m.validate()?;
        Ok(m)
    }

    pub fn get(&self, id: &str) -> Result<&ModelEntry, ModelError> {
        self.models
            .iter()
            .find(|m| m.id == id)
            .ok_or_else(|| ModelError::Unknown(id.to_string()))
    }

    fn validate(&self) -> Result<(), ModelError> {
        let bad = |m: &ModelEntry, why: &str| ModelError::Manifest(format!("{}: {why}", m.id));
        let mut seen = std::collections::HashSet::new();
        for m in &self.models {
            if !seen.insert(&m.id) {
                return Err(bad(m, "duplicate id"));
            }
            if m.sha256.len() != 64
                || !m
                    .sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err(bad(m, "sha256 must be 64 lowercase hex digits"));
            }
            if m.file.is_some() == m.archive.is_some() {
                return Err(bad(m, "exactly one of `file` and `archive` is required"));
            }
            if let Some(a) = &m.archive
                && a != "tar.bz2"
            {
                return Err(bad(m, "unsupported archive type"));
            }
            if m.size == 0 {
                return Err(bad(m, "size must be set"));
            }
            if !m.url.starts_with("https://") {
                return Err(bad(m, "url must be https"));
            }
            if m.id.is_empty() || m.id.contains(['/', '\\']) || m.id.starts_with('.') {
                return Err(bad(m, "id must not contain path separators"));
            }
            for f in m.file.iter().chain(m.required_files.iter()) {
                if f.starts_with('/') || f.split('/').any(|p| p == "..") {
                    return Err(bad(m, "unsafe file path"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_manifest_is_valid_and_pinned() {
        let m = Manifest::embedded().expect("valid");
        assert!(m.models.len() >= 5);
        let w = m.get("whistle").expect("whistle");
        assert_eq!(w.size, 16_919_407);
        assert!(
            w.url.contains("ca5287601bef25af26dcf2e1b2bdc0843a7c19e5"),
            "pinned to a commit"
        );
        assert!(
            m.get("parakeet-tdt-0.6b-v3-int8")
                .expect("p")
                .needs_attribution()
        );
        assert!(m.get("nope").is_err());
    }

    #[test]
    fn rejects_bad_entries() {
        let base = |extra: &str| {
            format!(
                "[[model]]\nid='a'\nkind='stt'\ntitle='t'\nurl='https://x/y'\nsha256='{}'\nsize=1\nlicense='MIT'\n{extra}",
                "a".repeat(64)
            )
        };
        assert!(Manifest::parse(&base("file='f'")).is_ok());
        assert!(
            Manifest::parse(&base("")).is_err(),
            "neither file nor archive"
        );
        assert!(Manifest::parse(&base("file='f'\narchive='tar.bz2'")).is_err());
        assert!(Manifest::parse(&base("archive='zip'")).is_err());
        assert!(Manifest::parse(&base("file='../x'")).is_err());
        assert!(Manifest::parse(&base("file='f'").replace("https://", "http://")).is_err());
        assert!(Manifest::parse(&base("file='f'").replace(&"a".repeat(64), "XYZ")).is_err());
        let dup = format!("{}\n{}", base("file='f'"), base("file='f'"));
        assert!(Manifest::parse(&dup).is_err());
    }
}
