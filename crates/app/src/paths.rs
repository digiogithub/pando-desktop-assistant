//! Platform directories for the app (config, data, cache, logs).
//!
//! Pattern from Bitácora's `paths.rs` (MIT).

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

/// Per-user directories used by the desktop app, from
/// `ProjectDirs::from("dev", "Pando", "pando-assistant")`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    /// App settings (`settings.json`).
    pub config_dir: PathBuf,
    /// Persistent app data.
    pub data_dir: PathBuf,
    /// Disposable cache.
    pub cache_dir: PathBuf,
    /// Rolling log files (inside the cache dir).
    pub log_dir: PathBuf,
}

/// Errors resolving or creating the app directories.
#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    /// The OS did not provide a home directory.
    #[error("cannot determine the platform directories (no home directory)")]
    NoHome,
    /// Creating a directory failed.
    #[error("cannot create directory {path}: {source}")]
    Create {
        /// The directory that could not be created.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
}

impl AppDirs {
    /// Resolves the directories for the current user.
    pub fn from_project_dirs() -> Result<Self, PathsError> {
        let dirs =
            ProjectDirs::from("dev", "Pando", "pando-assistant").ok_or(PathsError::NoHome)?;
        let cache_dir = dirs.cache_dir().to_path_buf();
        Ok(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
            log_dir: cache_dir.join("logs"),
            cache_dir,
        })
    }

    /// Lays the directories out under one root (tests, smoke runs).
    #[must_use]
    pub fn with_root(root: &Path) -> Self {
        let cache_dir = root.join("cache");
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            log_dir: cache_dir.join("logs"),
            cache_dir,
        }
    }

    /// Creates every directory (idempotent).
    pub fn ensure(&self) -> Result<(), PathsError> {
        for path in [
            &self.config_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.log_dir,
        ] {
            std::fs::create_dir_all(path).map_err(|source| PathsError::Create {
                path: path.clone(),
                source,
            })?;
        }
        Ok(())
    }

    /// `<config_dir>/settings.json`.
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_dirs_are_resolved_and_logs_live_in_cache() {
        let dirs = AppDirs::from_project_dirs().expect("home directory available");
        assert!(dirs.log_dir.starts_with(&dirs.cache_dir));
        assert!(
            dirs.config_dir
                .to_string_lossy()
                .to_lowercase()
                .contains("pando-assistant")
        );
    }

    #[test]
    fn ensure_creates_all_directories_idempotently() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dirs = AppDirs::with_root(tmp.path());
        dirs.ensure().expect("first ensure");
        dirs.ensure().expect("second ensure");
        for dir in [
            &dirs.config_dir,
            &dirs.data_dir,
            &dirs.cache_dir,
            &dirs.log_dir,
        ] {
            assert!(dir.is_dir(), "{} missing", dir.display());
        }
        assert_eq!(
            dirs.settings_file(),
            tmp.path().join("config/settings.json")
        );
    }
}
