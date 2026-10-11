//! Application settings persisted as JSON in `<config_dir>/settings.json`.
//!
//! Loading is tolerant: a missing file gives defaults, unknown keys are ignored, missing keys
//! take their default, and a file that does not parse is moved aside to a timestamped backup
//! so the app starts with defaults instead of failing. Saving is atomic (temp file in the same
//! directory, then rename).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Current layout of `settings.json`.
pub const SETTINGS_VERSION: u32 = 1;

/// Persisted application settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Layout version of the file.
    pub settings_version: u32,
    /// Explicit UI language tag (`es`, `en`); `None` follows the operating system.
    pub language: Option<String>,
    /// Manual Pando URL (skips discovery); `None` discovers the local instance.
    pub pando_url: Option<String>,
    /// Disable decorative animation (organism idle motion, transitions).
    pub reduce_motion: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            settings_version: SETTINGS_VERSION,
            language: None,
            pando_url: None,
            reduce_motion: false,
        }
    }
}

/// How [`AppSettings::load_or_default`] obtained the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadOutcome {
    /// Read from the file.
    Loaded,
    /// No file yet: defaults.
    Missing,
    /// The file was corrupt: it was moved to `backup` and defaults are used.
    Recovered {
        /// Where the corrupt file now lives (`None` if even the rename failed).
        backup: Option<PathBuf>,
    },
}

impl AppSettings {
    /// Loads the settings, recovering from a corrupt file with defaults plus a backup.
    #[must_use]
    pub fn load_or_default(path: &Path) -> (Self, LoadOutcome) {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return (Self::default(), LoadOutcome::Missing);
            }
            Err(err) => {
                tracing::warn!("cannot read {}: {err}", path.display());
                return (
                    Self::default(),
                    LoadOutcome::Recovered {
                        backup: backup_corrupt(path),
                    },
                );
            }
        };
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(settings) => (settings, LoadOutcome::Loaded),
            Err(err) => {
                tracing::warn!("{} is corrupt, using defaults: {err}", path.display());
                (
                    Self::default(),
                    LoadOutcome::Recovered {
                        backup: backup_corrupt(path),
                    },
                )
            }
        }
    }

    /// Writes the settings atomically.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        write_atomic(path, &json)
    }
}

/// Moves a corrupt settings file to `<name>.corrupt-<unix-seconds>`; returns the new path.
fn backup_corrupt(path: &Path) -> Option<PathBuf> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let name = path.file_name()?.to_string_lossy().into_owned();
    let backup = path.with_file_name(format!("{name}.corrupt-{stamp}"));
    match std::fs::rename(path, &backup) {
        Ok(()) => Some(backup),
        Err(err) => {
            tracing::warn!("cannot back up {}: {err}", path.display());
            None
        }
    }
}

/// Atomic write for app-owned files: temp file next to the target, fsync, rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
    let tmp = dir.join(format!("{name}.tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let (settings, outcome) = AppSettings::load_or_default(&tmp.path().join("settings.json"));
        assert_eq!(settings, AppSettings::default());
        assert_eq!(outcome, LoadOutcome::Missing);
    }

    #[test]
    fn save_then_load_round_trips() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("nested/settings.json");
        let settings = AppSettings {
            language: Some("en".into()),
            reduce_motion: true,
            ..AppSettings::default()
        };
        settings.save(&path).expect("save");
        let (loaded, outcome) = AppSettings::load_or_default(&path);
        assert_eq!(loaded, settings);
        assert_eq!(outcome, LoadOutcome::Loaded);
        // No temp file is left behind.
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().expect("parent"))
            .expect("read_dir")
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn unknown_and_missing_keys_are_tolerated() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, r#"{"language":"es","future_key":{"a":1}}"#).expect("write");
        let (settings, outcome) = AppSettings::load_or_default(&path);
        assert_eq!(settings.language.as_deref(), Some("es"));
        assert!(!settings.reduce_motion);
        assert_eq!(outcome, LoadOutcome::Loaded);
    }

    #[test]
    fn corrupt_settings_start_with_defaults_and_keep_a_backup() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{ not json").expect("write");
        let (settings, outcome) = AppSettings::load_or_default(&path);
        assert_eq!(settings, AppSettings::default());
        let LoadOutcome::Recovered {
            backup: Some(backup),
        } = outcome
        else {
            panic!("expected a recovery with backup, got {outcome:?}");
        };
        assert_eq!(
            std::fs::read_to_string(&backup).expect("backup"),
            "{ not json"
        );
        assert!(!path.exists(), "the corrupt file must be moved aside");
        // The next save writes a healthy file.
        settings.save(&path).expect("save");
        assert_eq!(AppSettings::load_or_default(&path).1, LoadOutcome::Loaded);
    }

    #[test]
    fn wrongly_typed_values_count_as_corrupt() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, r#"{"reduce_motion":"yes"}"#).expect("write");
        let (settings, outcome) = AppSettings::load_or_default(&path);
        assert_eq!(settings, AppSettings::default());
        assert!(matches!(outcome, LoadOutcome::Recovered { .. }));
    }
}
