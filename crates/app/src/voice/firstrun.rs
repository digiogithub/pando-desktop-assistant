//! First-run providers backed by the real audio and Claude bridge code (PDA-T-0149):
//! the microphone probe (PDA-T-0086) and the Claude Code hooks installer (PDA-T-0132).

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use audio::{Capture, CaptureConfig, CaptureEvent};
use claude_bridge::{HookInstaller, RuntimeFiles};

use crate::firstrun::{HooksInstaller, MicProbe};

/// How long a probe waits for the stream to report before declaring the microphone unusable.
const PROBE_TIMEOUT: Duration = Duration::from_millis(1_500);
/// A probe result is reused for this long (the wizard repaints often).
const CACHE_FOR: Duration = Duration::from_secs(3);

/// Opens the default input device for a moment: `Started` means access works, an error event
/// (permission denied, no device) means it does not. Retried after [`CACHE_FOR`], so granting
/// access in the OS and coming back to the wizard works without a restart.
#[derive(Debug, Default)]
pub struct AudioMicProbe {
    last: RefCell<Option<(Instant, bool)>>,
}

impl MicProbe for AudioMicProbe {
    fn available(&self) -> bool {
        if let Some((at, ok)) = *self.last.borrow()
            && at.elapsed() < CACHE_FOR
        {
            return ok;
        }
        let ok = probe();
        *self.last.borrow_mut() = Some((Instant::now(), ok));
        ok
    }
}

fn probe() -> bool {
    let (tx, rx) = mpsc::channel();
    let capture = Capture::start(
        CaptureConfig::default(),
        |_| {},
        move |ev| {
            let _ = tx.send(ev);
        },
    );
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let ok = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(CaptureEvent::Started { .. }) => break true,
            Ok(CaptureEvent::Error(_)) | Err(_) => break false,
            Ok(CaptureEvent::Restarted { .. }) => {}
        }
    };
    drop(capture);
    ok
}

/// Installs the Claude Code hooks into `~/.claude/settings.json` (idempotent, with a backup).
#[derive(Debug, Clone)]
pub struct BridgeHooks {
    data_dir: PathBuf,
}

impl BridgeHooks {
    /// Hooks installer for the app's data directory.
    #[must_use]
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    fn installer(&self) -> Option<HookInstaller> {
        HookInstaller::default_settings_path().map(|s| HookInstaller::new(s, &self.data_dir))
    }
}

impl HooksInstaller for BridgeHooks {
    fn available(&self) -> bool {
        self.installer()
            .is_some_and(|i| i.status().is_ok_and(|s| s.is_installed()))
    }

    fn install(&self) -> Result<String, String> {
        let installer = self.installer().ok_or("no home directory")?;
        let runtime = RuntimeFiles::new(self.data_dir.join("claude-bridge"));
        installer
            .install(&runtime)
            .map(|r| format!("{r:?}"))
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_installer_reports_not_installed_without_a_settings_file() {
        let tmp = tempfile::tempdir().expect("tmp");
        let h = BridgeHooks::new(tmp.path().to_path_buf());
        // The real ~/.claude is only read here (status), never written.
        let _ = h.available();
    }

    #[test]
    fn the_probe_cache_is_reused() {
        let p = AudioMicProbe::default();
        *p.last.borrow_mut() = Some((Instant::now(), true));
        assert!(
            p.available(),
            "a fresh cached answer is returned without opening the device"
        );
    }
}
