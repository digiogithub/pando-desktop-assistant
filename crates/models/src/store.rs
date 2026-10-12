//! The models directory: install, verify, import.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::download::{self, Cancel, Progress, Transport, sha256_file};
use crate::{MODELS_DIR_ENV, Manifest, ModelEntry, ModelError};

const MARKER: &str = ".pando-model";

/// Installation state of one model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelState {
    Missing,
    Ok,
    /// Present but not usable (reason).
    Corrupt(String),
}

/// A models directory plus the manifest describing what belongs in it.
#[derive(Debug, Clone)]
pub struct ModelStore {
    root: PathBuf,
    manifest: Manifest,
}

impl ModelStore {
    /// `PANDO_ASSISTANT_MODELS_DIR`, else the per-user data directory, with the embedded manifest.
    pub fn open() -> Result<Self, ModelError> {
        Ok(Self::at(Self::default_dir(), Manifest::embedded()?))
    }

    pub fn at(root: impl Into<PathBuf>, manifest: Manifest) -> Self {
        Self {
            root: root.into(),
            manifest,
        }
    }

    /// Env override, else `<data dir>/models` (Linux `~/.local/share/pando-assistant/models`).
    pub fn default_dir() -> PathBuf {
        if let Some(d) = std::env::var_os(MODELS_DIR_ENV)
            && !d.is_empty()
        {
            return d.into();
        }
        ProjectDirs::from("dev", "Pando", "pando-assistant")
            .map_or_else(|| PathBuf::from("models"), |p| p.data_dir().join("models"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn dir(&self, e: &ModelEntry) -> PathBuf {
        self.root.join(&e.id)
    }

    /// Where the model is (single file: the file; archive: the directory). Does not check existence.
    pub fn path(&self, id: &str) -> Result<PathBuf, ModelError> {
        let e = self.manifest.get(id)?;
        Ok(match &e.file {
            Some(f) => self.dir(e).join(f),
            None => self.dir(e),
        })
    }

    fn marker(&self, e: &ModelEntry) -> PathBuf {
        self.dir(e).join(MARKER)
    }

    fn write_marker(&self, e: &ModelEntry) -> Result<(), ModelError> {
        let mut text = format!("sha256={}\nsize={}\n", e.sha256, e.size);
        let files: Vec<String> = match &e.file {
            Some(f) => vec![f.clone()],
            None => e.required_files.clone(),
        };
        for f in files {
            let len = std::fs::metadata(self.dir(e).join(&f))?.len();
            text.push_str(&format!("file={f}\t{len}\n"));
        }
        std::fs::write(self.marker(e), text)?;
        Ok(())
    }

    /// State of one model. `deep` re-hashes single-file models.
    pub fn verify(&self, id: &str, deep: bool) -> Result<ModelState, ModelError> {
        let e = self.manifest.get(id)?;
        let Ok(marker) = std::fs::read_to_string(self.marker(e)) else {
            return Ok(if self.dir(e).exists() {
                ModelState::Corrupt("no install marker".into())
            } else {
                ModelState::Missing
            });
        };
        let mut sha_ok = false;
        for line in marker.lines() {
            if let Some(v) = line.strip_prefix("sha256=") {
                sha_ok = v == e.sha256;
            } else if let Some(v) = line.strip_prefix("file=")
                && let Some((rel, len)) = v.split_once('\t')
            {
                match std::fs::metadata(self.dir(e).join(rel)) {
                    Ok(m) if m.len().to_string() == len => {}
                    Ok(_) => return Ok(ModelState::Corrupt(format!("{rel} has the wrong size"))),
                    Err(_) => return Ok(ModelState::Corrupt(format!("{rel} is missing"))),
                }
            }
        }
        if !sha_ok {
            return Ok(ModelState::Corrupt(
                "installed from a different version".into(),
            ));
        }
        for f in &e.required_files {
            if !self.dir(e).join(f).is_file() {
                return Ok(ModelState::Corrupt(format!("{f} is missing")));
            }
        }
        if deep && let Some(f) = &e.file {
            let got = sha256_file(&self.dir(e).join(f))?;
            if got != e.sha256 {
                return Ok(ModelState::Corrupt("SHA-256 does not match".into()));
            }
        }
        Ok(ModelState::Ok)
    }

    /// Every manifest entry with its state.
    pub fn list(&self) -> Vec<(ModelEntry, ModelState)> {
        self.manifest
            .models
            .iter()
            .map(|e| {
                let s = self
                    .verify(&e.id, false)
                    .unwrap_or_else(|err| ModelState::Corrupt(err.to_string()));
                (e.clone(), s)
            })
            .collect()
    }

    /// Download (resuming) and install `id`. A wrong hash deletes the download and fails.
    pub fn install(
        &self,
        id: &str,
        transport: &dyn Transport,
        cancel: &Cancel,
        on_progress: &mut dyn FnMut(Progress),
    ) -> Result<PathBuf, ModelError> {
        let e = self.manifest.get(id)?.clone();
        std::fs::create_dir_all(&self.root)?;
        let staged = self.root.join(format!("{}.download", e.id));
        download::download(&e, &staged, transport, cancel, on_progress)?;
        self.finish_install(&e, &staged)?;
        self.path(id)
    }

    /// Move a verified download into place and write the marker.
    fn finish_install(&self, e: &ModelEntry, verified: &Path) -> Result<(), ModelError> {
        let dir = self.dir(e);
        match (&e.file, &e.archive) {
            (Some(f), _) => {
                std::fs::create_dir_all(&dir)?;
                let dest = dir.join(f);
                if std::fs::rename(verified, &dest).is_err() {
                    std::fs::copy(verified, &dest)?;
                    std::fs::remove_file(verified)?;
                }
            }
            (None, Some(_)) => {
                let tmp = self.root.join(format!("{}.tmp", e.id));
                if tmp.exists() {
                    std::fs::remove_dir_all(&tmp)?;
                }
                crate::unpack::unpack_tar_bz2(verified, &tmp)?;
                for f in &e.required_files {
                    if !tmp.join(f).is_file() {
                        std::fs::remove_dir_all(&tmp)?;
                        return Err(ModelError::Download {
                            id: e.id.clone(),
                            message: format!("the archive does not contain {f}"),
                        });
                    }
                }
                if dir.exists() {
                    std::fs::remove_dir_all(&dir)?;
                }
                std::fs::rename(&tmp, &dir)?;
                std::fs::remove_file(verified)?;
            }
            (None, None) => {
                return Err(ModelError::Manifest(format!(
                    "{}: no file or archive",
                    e.id
                )));
            }
        }
        self.write_marker(e)
    }

    /// Install every model found in `src` (an offline folder holding the files or archives by their
    /// download name). Each file is hash-verified first. Returns one result per model that was found.
    pub fn import(&self, src: &Path) -> Vec<(String, Result<(), ModelError>)> {
        let mut out = Vec::new();
        for e in &self.manifest.models {
            let name = e
                .url
                .rsplit('/')
                .next()
                .map(str::to_string)
                .unwrap_or_default();
            let candidate = [e.file.clone().unwrap_or_default(), name]
                .into_iter()
                .filter(|n| !n.is_empty())
                .map(|n| src.join(n))
                .find(|p| p.is_file());
            let Some(file) = candidate else { continue };
            let res = (|| {
                let got = sha256_file(&file)?;
                if got != e.sha256 {
                    return Err(ModelError::HashMismatch {
                        id: e.id.clone(),
                        expected: e.sha256.clone(),
                        got,
                    });
                }
                std::fs::create_dir_all(&self.root)?;
                let staged = self.root.join(format!("{}.download", e.id));
                std::fs::copy(&file, &staged)?;
                self.finish_install(e, &staged)
            })();
            out.push((e.id.clone(), res));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use super::*;
    use crate::download::tests::{Fake, entry_for};
    use crate::unpack::tests::make_archive;

    fn no_cancel() -> Cancel {
        Arc::new(AtomicBool::new(false))
    }

    fn store_with(entries: Vec<ModelEntry>, dir: &Path) -> ModelStore {
        ModelStore::at(dir, Manifest { models: entries })
    }

    #[test]
    fn single_file_install_verify_and_corruption() {
        let data = vec![5u8; 10_000];
        let mut e = entry_for(&data);
        e.id = "m".into();
        let dir = tempfile::tempdir().expect("tmp");
        let s = store_with(vec![e], dir.path());
        assert_eq!(s.verify("m", false).expect("v"), ModelState::Missing);
        let p = s
            .install("m", &Fake::new(data.clone()), &no_cancel(), &mut |_| {})
            .expect("install");
        assert_eq!(p, dir.path().join("m/t.bin"));
        assert_eq!(s.verify("m", true).expect("v"), ModelState::Ok);
        // bit rot: same size, different bytes -> only the deep check notices
        std::fs::write(&p, vec![6u8; 10_000]).expect("write");
        assert_eq!(s.verify("m", false).expect("v"), ModelState::Ok);
        assert!(matches!(
            s.verify("m", true).expect("v"),
            ModelState::Corrupt(_)
        ));
        // truncation is caught by the cheap check
        std::fs::write(&p, b"x").expect("write");
        assert!(matches!(
            s.verify("m", false).expect("v"),
            ModelState::Corrupt(_)
        ));
        assert!(s.verify("nope", false).is_err());
    }

    #[test]
    fn wrong_hash_leaves_nothing_installed() {
        let data = vec![1u8; 3000];
        let mut e = entry_for(&data);
        e.id = "m".into();
        e.sha256 = "f".repeat(64);
        let dir = tempfile::tempdir().expect("tmp");
        let s = store_with(vec![e], dir.path());
        let err = s
            .install("m", &Fake::new(data), &no_cancel(), &mut |_| {})
            .expect_err("hash");
        assert!(matches!(err, ModelError::HashMismatch { .. }));
        assert_eq!(s.verify("m", false).expect("v"), ModelState::Missing);
        assert!(
            !dir.path().join("m.download").exists() && !dir.path().join("m.download.part").exists()
        );
    }

    fn archive_entry(archive: &Path) -> ModelEntry {
        let bytes = std::fs::read(archive).expect("read");
        let mut e = entry_for(&bytes);
        e.id = "voice".into();
        e.file = None;
        e.archive = Some("tar.bz2".into());
        e.url = "https://example.invalid/voice.tar.bz2".into();
        e.required_files = vec!["model.onnx".into(), "sub/data".into()];
        e
    }

    #[test]
    fn archive_install_import_and_missing_required_file() {
        let src = tempfile::tempdir().expect("src");
        let a = src.path().join("voice.tar.bz2");
        make_archive(&a, &[("v/model.onnx", b"M"), ("v/sub/data", b"D")]);
        let e = archive_entry(&a);
        let dir = tempfile::tempdir().expect("tmp");
        let s = store_with(vec![e.clone()], dir.path());
        // offline import
        let res = s.import(src.path());
        assert_eq!(res.len(), 1);
        assert!(res[0].1.is_ok(), "{:?}", res[0].1);
        assert_eq!(s.verify("voice", false).expect("v"), ModelState::Ok);
        assert_eq!(
            std::fs::read(s.path("voice").expect("p").join("sub/data")).expect("d"),
            b"D"
        );
        // removing a required file is detected
        std::fs::remove_file(dir.path().join("voice/model.onnx")).expect("rm");
        assert!(matches!(
            s.verify("voice", false).expect("v"),
            ModelState::Corrupt(_)
        ));
        // an archive that lacks a required file is refused
        let b = src.path().join("bad.tar.bz2");
        make_archive(&b, &[("v/model.onnx", b"M")]);
        let mut e2 = archive_entry(&b);
        e2.id = "bad".into();
        let s2 = store_with(vec![e2], tempfile::tempdir().expect("t").path());
        let bytes = std::fs::read(&b).expect("read");
        let err = s2
            .install("bad", &Fake::new(bytes), &no_cancel(), &mut |_| {})
            .expect_err("req");
        assert!(err.to_string().contains("sub/data"), "{err}");
    }

    #[test]
    fn import_rejects_a_tampered_file() {
        let src = tempfile::tempdir().expect("src");
        let a = src.path().join("voice.tar.bz2");
        make_archive(&a, &[("v/model.onnx", b"M"), ("v/sub/data", b"D")]);
        let e = archive_entry(&a);
        std::fs::write(&a, b"tampered").expect("tamper");
        let dir = tempfile::tempdir().expect("tmp");
        let s = store_with(vec![e], dir.path());
        let res = s.import(src.path());
        assert!(matches!(res[0].1, Err(ModelError::HashMismatch { .. })));
        assert_eq!(s.verify("voice", false).expect("v"), ModelState::Missing);
    }

    #[test]
    fn manifest_change_marks_models_stale() {
        let data = vec![8u8; 100];
        let mut e = entry_for(&data);
        e.id = "m".into();
        let dir = tempfile::tempdir().expect("tmp");
        let s = store_with(vec![e.clone()], dir.path());
        s.install("m", &Fake::new(data), &no_cancel(), &mut |_| {})
            .expect("install");
        let mut newer = e;
        newer.sha256 = "1".repeat(64);
        let s2 = store_with(vec![newer], dir.path());
        assert!(matches!(
            s2.verify("m", false).expect("v"),
            ModelState::Corrupt(_)
        ));
    }

    #[test]
    fn default_dir_honours_the_env_var() {
        // single test touching the variable
        // SAFETY: set_var is unsafe in edition 2024; this is the only test that touches it.
        unsafe { std::env::set_var(MODELS_DIR_ENV, "/tmp/pando-models-test") };
        assert_eq!(
            ModelStore::default_dir(),
            PathBuf::from("/tmp/pando-models-test")
        );
        unsafe { std::env::remove_var(MODELS_DIR_ENV) };
        assert!(ModelStore::default_dir().ends_with("models"));
    }
}
