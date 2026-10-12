//! [`ModelProvisioner`] backed by the `models` crate (PDA-T-0148): the user's model directory,
//! the pinned manifest, verified resumable downloads on a worker thread.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use models::{HttpTransport, ModelState, ModelStore};

use super::{ModelNeed, ModelProvisioner, NeedState};

/// Voice models the user can download (the bundled ones ship with the installer).
pub struct StoreProvisioner {
    store: ModelStore,
    states: Arc<Mutex<HashMap<String, NeedState>>>,
}

impl std::fmt::Debug for StoreProvisioner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StoreProvisioner({})", self.store.root().display())
    }
}

impl StoreProvisioner {
    /// The default store (`PANDO_ASSISTANT_MODELS_DIR` or the per-user data dir).
    pub fn open() -> Result<Self, String> {
        Ok(Self::with_store(
            ModelStore::open().map_err(|e| e.to_string())?,
        ))
    }

    /// Over an explicit store (tests).
    #[must_use]
    pub fn with_store(store: ModelStore) -> Self {
        Self {
            store,
            states: Arc::default(),
        }
    }
}

impl ModelProvisioner for StoreProvisioner {
    fn required(&self) -> Vec<ModelNeed> {
        self.store
            .manifest()
            .models
            .iter()
            .filter(|m| !m.bundled)
            .map(|m| ModelNeed {
                id: m.id.clone(),
                label: m.title.clone(),
                bytes: m.size,
                licence: m.license.clone(),
            })
            .collect()
    }

    fn state(&self, id: &str) -> NeedState {
        if let Some(s) = self.states.lock().ok().and_then(|m| m.get(id).cloned())
            && !matches!(s, NeedState::Ready)
        {
            return s;
        }
        match self.store.verify(id, false) {
            Ok(ModelState::Ok) => NeedState::Ready,
            _ => NeedState::Missing,
        }
    }

    fn install(&self, id: &str) -> Result<(), String> {
        self.store.manifest().get(id).map_err(|e| e.to_string())?;
        {
            let mut m = self.states.lock().map_err(|e| e.to_string())?;
            if matches!(m.get(id), Some(NeedState::Downloading(_))) {
                return Ok(());
            }
            m.insert(id.to_owned(), NeedState::Downloading(0));
        }
        let (store, states, id) = (self.store.clone(), self.states.clone(), id.to_owned());
        std::thread::Builder::new()
            .name("pando-model-download".into())
            .spawn(move || {
                let cancel = Arc::new(AtomicBool::new(false));
                let progress_states = states.clone();
                let progress_id = id.clone();
                let result = store.install(&id, &HttpTransport::new(), &cancel, &mut |p| {
                    let pct = (p.downloaded * 100).checked_div(p.total).unwrap_or(0) as u8;
                    if let Ok(mut m) = progress_states.lock() {
                        m.insert(progress_id.clone(), NeedState::Downloading(pct));
                    }
                });
                if let Ok(mut m) = states.lock() {
                    m.insert(
                        id,
                        match result {
                            Ok(_) => NeedState::Ready,
                            Err(e) => NeedState::Failed(e.to_string()),
                        },
                    );
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use models::Manifest;

    fn manifest() -> Manifest {
        Manifest::parse(
            r#"
[[model]]
id = "shipped"
kind = "stt"
title = "Shipped"
url = "https://example.invalid/a"
file = "a.bin"
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
size = 10
license = "MIT"
bundled = true

[[model]]
id = "voice"
kind = "tts"
title = "A voice"
url = "https://example.invalid/b"
file = "b.bin"
sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
size = 2048
license = "Apache-2.0"
"#,
        )
        .expect("manifest")
    }

    #[test]
    fn lists_only_downloadable_models_and_reports_missing() {
        let dir = tempfile::tempdir().expect("dir");
        let p = StoreProvisioner::with_store(ModelStore::at(dir.path(), manifest()));
        let needs = p.required();
        assert_eq!(needs.len(), 1);
        assert_eq!(
            (
                needs[0].id.as_str(),
                needs[0].bytes,
                needs[0].licence.as_str()
            ),
            ("voice", 2048, "Apache-2.0")
        );
        assert_eq!(p.state("voice"), NeedState::Missing);
        assert!(p.install("nope").is_err());
        assert_eq!(super::super::missing_bytes(&p), 2048);
    }

    #[test]
    fn the_embedded_manifest_loads() {
        let p = StoreProvisioner::open().expect("store");
        assert!(!p.required().is_empty());
    }
}
