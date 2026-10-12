//! Voice settings, persisted next to `settings.json` as `voice.json` (kept separate so the
//! voice front end owns its schema). Loading is tolerant, saving is atomic.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File name inside the config directory.
pub const FILE_NAME: &str = "voice.json";

/// Voice settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    /// Master switch: no worker is started and no microphone is opened while false.
    pub enabled: bool,
    /// Audio host (`PipeWire`, `PulseAudio`, ...); `None` = platform default.
    pub host: Option<String>,
    /// Input device id; `None` follows the system default.
    pub input_device: Option<String>,
    /// Output device id; `None` follows the system default.
    pub output_device: Option<String>,
    /// STT model file (Whistle `.cact`) or directory (Parakeet); `None` searches the usual places.
    pub stt_model: Option<PathBuf>,
    /// TTS model directory (sherpa-onnx Piper/Kokoro); `None` disables speech output.
    pub tts_model_dir: Option<PathBuf>,
    /// TTS voice name.
    pub tts_voice: String,
    /// Forced recognition language (ADR 0008).
    pub language: String,
    /// Speech rate multiplier.
    pub speed: f32,
    /// Speak answers aloud.
    pub speak: bool,
    /// Speak routine notifications (agent finished, ...), not only direct answers.
    pub speak_notifications: bool,
    /// Hands-free listening ("Oye Pando"): the microphone streams to the STT worker's VAD and
    /// wake verification. Off by default; nothing is stored.
    pub hands_free: bool,
    /// Wake sensitivity, 0.0 (strict) ..= 1.0 (permissive).
    pub wake_sensitivity: f32,
    /// Let the user interrupt the assistant by speaking (needs echo cancellation or a headset).
    pub barge_in: bool,
    /// The user wears a headset: hands-free barge-in works without echo cancellation.
    pub headset: bool,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: None,
            input_device: None,
            output_device: None,
            stt_model: None,
            tts_model_dir: None,
            tts_voice: String::new(),
            language: "es".to_owned(),
            speed: 1.0,
            speak: true,
            speak_notifications: true,
            hands_free: false,
            wake_sensitivity: 0.5,
            barge_in: true,
            headset: false,
        }
    }
}

impl VoiceConfig {
    /// Reads the file; anything unreadable yields defaults.
    #[must_use]
    pub fn load(config_dir: &Path) -> Self {
        std::fs::read(config_dir.join(FILE_NAME))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Writes the file atomically.
    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let tmp = config_dir.join(format!("{FILE_NAME}.tmp"));
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, config_dir.join(FILE_NAME))
    }

    /// The STT model to load: the configured one, `$PANDO_STT_MODEL`, the data dir, or the
    /// repo's `vendor/needle/models/whistle.cact` (development).
    #[must_use]
    pub fn resolve_stt_model(&self, data_dir: &Path) -> Option<PathBuf> {
        let env = std::env::var_os("PANDO_STT_MODEL").map(PathBuf::from);
        [
            self.stt_model.clone(),
            env,
            Some(data_dir.join("models").join("whistle.cact")),
            Some(PathBuf::from("vendor/needle/models/whistle.cact")),
        ]
        .into_iter()
        .flatten()
        .find(|p| p.exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_garbage() {
        let dir = tempfile::tempdir().expect("tmp");
        assert_eq!(VoiceConfig::load(dir.path()), VoiceConfig::default());
        let cfg = VoiceConfig {
            enabled: true,
            input_device: Some("usb".into()),
            ..VoiceConfig::default()
        };
        cfg.save(dir.path()).expect("save");
        assert_eq!(VoiceConfig::load(dir.path()), cfg);
        std::fs::write(dir.path().join(FILE_NAME), b"{nope").expect("write");
        assert_eq!(VoiceConfig::load(dir.path()), VoiceConfig::default());
    }

    #[test]
    fn voice_is_off_by_default_and_the_model_is_found_in_the_data_dir() {
        assert!(!VoiceConfig::default().enabled);
        let dir = tempfile::tempdir().expect("tmp");
        let cfg = VoiceConfig::default();
        assert!(
            cfg.resolve_stt_model(dir.path()).is_none()
                || std::env::var_os("PANDO_STT_MODEL").is_some()
        );
        std::fs::create_dir_all(dir.path().join("models")).expect("mk");
        std::fs::write(dir.path().join("models/whistle.cact"), b"x").expect("w");
        assert!(cfg.resolve_stt_model(dir.path()).is_some());
    }
}
