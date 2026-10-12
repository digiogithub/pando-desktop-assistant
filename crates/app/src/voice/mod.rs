//! Voice front end of the app: worker supervision, microphone levels, push-to-talk and the
//! orchestration of speech in and out (ADR 0007: the app only talks to the workers over
//! `voice-proto` pipes and never links sherpa-onnx).

pub mod config;
pub mod controller;
pub mod devices;
pub mod dock;
pub mod latency;
pub mod levels;
pub mod ptt;
pub mod resolve;
pub mod service;
pub mod session;
mod settings_panel;
pub mod speak;
pub mod supervisor;
pub mod wer;

/// The voice configuration and directories at startup. Tests and the smoke run never start
/// workers or open the microphone.
#[must_use]
pub fn startup_config() -> (config::VoiceConfig, controller::VoiceDirs) {
    let dirs = crate::paths::AppDirs::from_project_dirs().ok();
    let data_dir = dirs
        .as_ref()
        .map(|d| d.data_dir.clone())
        .unwrap_or_default();
    if cfg!(test) || std::env::var_os("PANDO_ASSISTANT_NO_VOICE").is_some() {
        let voice_dirs = controller::VoiceDirs {
            config_dir: None,
            data_dir,
        };
        return (config::VoiceConfig::default(), voice_dirs);
    }
    let config_dir = dirs.map(|d| d.config_dir);
    let cfg = config_dir
        .as_deref()
        .map(config::VoiceConfig::load)
        .unwrap_or_default();
    (
        cfg,
        controller::VoiceDirs {
            config_dir,
            data_dir,
        },
    )
}
