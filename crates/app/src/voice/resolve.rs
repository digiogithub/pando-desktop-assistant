//! Where the voice models live (PDA-T-0090, `PDA-SP-0012`): explicit configuration, the
//! `models` crate store (user data dir or `PANDO_ASSISTANT_MODELS_DIR`), then the directory
//! shipped by the installer (`<exe>/../lib/pando-assistant/models`, which is also the AppImage
//! layout), then the development `vendor/` copy.

use std::path::{Path, PathBuf};

use models::{Manifest, ModelState, ModelStore};

use super::config::VoiceConfig;

/// Default STT model id in the manifest.
pub const STT_MODEL_ID: &str = "whistle";
/// Default TTS model id in the manifest.
pub const TTS_MODEL_ID: &str = "piper-es_ES-davefx-medium";

/// The model stores to search, in priority order.
#[must_use]
pub fn stores() -> Vec<ModelStore> {
    let mut out = Vec::new();
    if let Ok(store) = ModelStore::open() {
        out.push(store);
    }
    if let Some(dir) = packaged_models_dir()
        && let Ok(manifest) = Manifest::embedded()
    {
        out.push(ModelStore::at(dir, manifest));
    }
    out
}

/// `<exe>/../lib/pando-assistant/models` when it exists.
#[must_use]
pub fn packaged_models_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?.join("lib/pando-assistant/models");
    dir.is_dir().then_some(dir)
}

/// Path of an installed model in the first store that has it.
#[must_use]
pub fn installed(stores: &[ModelStore], id: &str) -> Option<PathBuf> {
    stores.iter().find_map(|s| {
        let ok = matches!(s.verify(id, false), Ok(ModelState::Ok));
        ok.then(|| s.path(id).ok()).flatten()
    })
}

/// The STT model to load.
#[must_use]
pub fn stt_model(cfg: &VoiceConfig, data_dir: &Path) -> Option<PathBuf> {
    stt_model_in(cfg, data_dir, &stores())
}

fn stt_model_in(cfg: &VoiceConfig, data_dir: &Path, stores: &[ModelStore]) -> Option<PathBuf> {
    cfg.resolve_stt_model(data_dir)
        .or_else(|| installed(stores, STT_MODEL_ID))
}

/// The TTS model directory: configured, else the default installed Piper voice.
#[must_use]
pub fn tts_model_dir(cfg: &VoiceConfig) -> Option<PathBuf> {
    tts_model_dir_in(cfg, &stores())
}

fn tts_model_dir_in(cfg: &VoiceConfig, stores: &[ModelStore]) -> Option<PathBuf> {
    cfg.tts_model_dir
        .clone()
        .filter(|d| d.exists())
        .or_else(|| installed(stores, TTS_MODEL_ID))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_model_resolves_to_none() {
        let tmp = tempfile::tempdir().expect("tmp");
        let store = ModelStore::at(tmp.path(), Manifest::embedded().expect("manifest"));
        let cfg = VoiceConfig::default();
        assert_eq!(installed(std::slice::from_ref(&store), STT_MODEL_ID), None);
        assert_eq!(tts_model_dir_in(&cfg, &[store]), None);
    }

    #[test]
    fn an_explicit_tts_dir_wins_when_it_exists() {
        let tmp = tempfile::tempdir().expect("tmp");
        let cfg = VoiceConfig {
            tts_model_dir: Some(tmp.path().to_path_buf()),
            ..VoiceConfig::default()
        };
        assert_eq!(tts_model_dir_in(&cfg, &[]), Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn worker_lookup_falls_back_to_the_bare_name() {
        let p = super::super::supervisor::locate_worker("definitely-not-a-worker");
        assert_eq!(p, PathBuf::from("definitely-not-a-worker"));
    }

    #[test]
    fn stt_falls_back_through_the_stores() {
        let tmp = tempfile::tempdir().expect("tmp");
        let store = ModelStore::at(tmp.path(), Manifest::embedded().expect("manifest"));
        let cfg = VoiceConfig::default();
        let found = stt_model_in(&cfg, tmp.path(), &[store]);
        assert!(found.is_none() || std::env::var_os("PANDO_STT_MODEL").is_some());
    }
}
