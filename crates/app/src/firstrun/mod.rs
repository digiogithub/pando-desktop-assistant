//! First-run assistant (PDA-T-0149): connection, microphone, voice models, hotkey, optional
//! Claude Code hooks and a "say hello" demo. It can be skipped at any step and reopened from the
//! settings (`Command::OpenFirstRun`).
//!
//! The pure wizard state ([`Wizard`]) is tested here. The things other work packages provide are
//! behind traits with honest stubs until they land: [`ModelProvisioner`] (voice models; will bind
//! to the `models` crate of PDA-T-0148 once it is on `main`), [`MicProbe`] (audio capture,
//! PDA-US-0025) and [`HooksInstaller`] (claude-bridge).

mod store;
mod view;

pub use store::StoreProvisioner;
pub use view::{FirstRunView, open};

use std::collections::BTreeSet;

/// The steps, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    /// Find or enter the Pando address.
    Connection,
    /// Microphone permission and test.
    Microphone,
    /// Choose / download the voice and STT models.
    Voice,
    /// Hold-to-talk shortcut (portal on Wayland).
    Hotkey,
    /// Optional Claude Code hooks.
    ClaudeHooks,
    /// "Di hola" demo.
    Hello,
}

/// All steps in display order.
pub const STEPS: [Step; 6] = [
    Step::Connection,
    Step::Microphone,
    Step::Voice,
    Step::Hotkey,
    Step::ClaudeHooks,
    Step::Hello,
];

impl Step {
    /// Locale id (`firstrun.step.<id>`).
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Connection => "connection",
            Self::Microphone => "microphone",
            Self::Voice => "voice",
            Self::Hotkey => "hotkey",
            Self::ClaudeHooks => "hooks",
            Self::Hello => "hello",
        }
    }

    /// Steps the user may skip without consequence.
    #[must_use]
    pub fn optional(self) -> bool {
        matches!(self, Self::ClaudeHooks | Self::Hello | Self::Voice)
    }
}

/// Position and progress of the wizard.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wizard {
    index: usize,
    done: BTreeSet<Step>,
    skipped: BTreeSet<Step>,
    finished: bool,
}

impl Wizard {
    /// At the first step.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current step (the last one once finished).
    #[must_use]
    pub fn step(&self) -> Step {
        STEPS[self.index.min(STEPS.len() - 1)]
    }

    /// 1-based position and total, for "2 / 6".
    #[must_use]
    pub fn position(&self) -> (usize, usize) {
        (self.index.min(STEPS.len() - 1) + 1, STEPS.len())
    }

    /// Whether the wizard ended (completed or skipped).
    #[must_use]
    pub fn finished(&self) -> bool {
        self.finished
    }

    /// Marks the current step done and moves on; finishing after the last step.
    pub fn complete_step(&mut self) {
        self.done.insert(self.step());
        self.advance();
    }

    /// Skips the current step.
    pub fn skip_step(&mut self) {
        self.skipped.insert(self.step());
        self.advance();
    }

    /// Back one step.
    pub fn back(&mut self) {
        self.finished = false;
        self.index = self.index.saturating_sub(1);
    }

    /// Leave the wizard now ("Saltar todo").
    pub fn skip_all(&mut self) {
        for s in STEPS.iter().skip(self.index) {
            self.skipped.insert(*s);
        }
        self.finished = true;
    }

    /// Steps that were completed.
    #[must_use]
    pub fn done(&self) -> &BTreeSet<Step> {
        &self.done
    }

    /// Steps that were skipped.
    #[must_use]
    pub fn skipped(&self) -> &BTreeSet<Step> {
        &self.skipped
    }

    fn advance(&mut self) {
        if self.index + 1 >= STEPS.len() {
            self.finished = true;
        } else {
            self.index += 1;
        }
    }
}

// ------------------------------------------------------------------- provider traits

/// A model the voice features need, with what the consent screen shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelNeed {
    /// Manifest id.
    pub id: String,
    /// Human name.
    pub label: String,
    /// Download size in bytes.
    pub bytes: u64,
    /// Licence name.
    pub licence: String,
}

/// State of one model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NeedState {
    /// Not downloaded.
    Missing,
    /// Downloading, percent.
    Downloading(u8),
    /// Installed and verified.
    Ready,
    /// Failed, with a reason.
    Failed(String),
}

/// Model download service (PDA-T-0148 `models` crate binds here).
pub trait ModelProvisioner {
    /// Models the user should have.
    fn required(&self) -> Vec<ModelNeed>;
    /// State of `id`.
    fn state(&self, id: &str) -> NeedState;
    /// Starts downloading `id` (after the user consented); progress is read through `state`.
    fn install(&self, id: &str) -> Result<(), String>;
}

/// Used until the `models` crate is wired: nothing to download.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoModels;

impl ModelProvisioner for NoModels {
    fn required(&self) -> Vec<ModelNeed> {
        Vec::new()
    }
    fn state(&self, _id: &str) -> NeedState {
        NeedState::Missing
    }
    fn install(&self, _id: &str) -> Result<(), String> {
        Err("model downloads are not available in this build".into())
    }
}

/// Microphone permission / level test (audio capture, PDA-US-0025).
pub trait MicProbe {
    /// Whether this build can capture audio at all.
    fn available(&self) -> bool;
}

/// Stub: no capture backend yet.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoMic;

impl MicProbe for NoMic {
    fn available(&self) -> bool {
        false
    }
}

/// Claude Code hooks installer (claude-bridge).
pub trait HooksInstaller {
    /// Whether the hooks are installed (or can be, for stubs).
    fn available(&self) -> bool;
    /// Installs the hooks (with the user's consent). `Ok` carries a short report.
    fn install(&self) -> Result<String, String> {
        Err("not supported".to_owned())
    }
}

/// Stub: claude-bridge is not wired into the app yet.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHooks;

impl HooksInstaller for NoHooks {
    fn available(&self) -> bool {
        false
    }
}

/// Total download size of the models still missing.
#[must_use]
pub fn missing_bytes(p: &dyn ModelProvisioner) -> u64 {
    p.required()
        .iter()
        .filter(|m| p.state(&m.id) != NeedState::Ready)
        .map(|m| m.bytes)
        .sum()
}

/// "123.4 MB" style size.
#[must_use]
pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = b as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_all_steps_and_finishes() {
        let mut w = Wizard::new();
        assert_eq!((w.step(), w.position()), (Step::Connection, (1, 6)));
        for _ in 0..5 {
            w.complete_step();
            assert!(!w.finished());
        }
        assert_eq!(w.step(), Step::Hello);
        w.complete_step();
        assert!(w.finished());
        assert_eq!(w.done().len(), 6);
    }

    #[test]
    fn can_skip_a_step_go_back_and_skip_everything() {
        let mut w = Wizard::new();
        w.complete_step();
        w.skip_step();
        assert!(w.skipped().contains(&Step::Microphone));
        w.back();
        assert_eq!(w.step(), Step::Microphone);
        w.skip_all();
        assert!(w.finished());
        assert!(w.skipped().contains(&Step::Hello) && w.done().len() == 1);
        w.back();
        assert!(!w.finished(), "going back reopens the wizard");
    }

    #[test]
    fn only_non_essential_steps_are_optional() {
        assert!(!Step::Connection.optional() && !Step::Hotkey.optional());
        assert!(Step::ClaudeHooks.optional() && Step::Voice.optional());
    }

    #[test]
    fn stubs_are_honest_and_sizes_are_readable() {
        let p = NoModels;
        assert!(p.required().is_empty() && p.install("x").is_err());
        assert_eq!(missing_bytes(&p), 0);
        assert!(!NoMic.available() && !NoHooks.available());
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(150 * 1024 * 1024), "150.0 MB");
    }

    struct Fake;
    impl ModelProvisioner for Fake {
        fn required(&self) -> Vec<ModelNeed> {
            vec![
                ModelNeed {
                    id: "a".into(),
                    label: "A".into(),
                    bytes: 100,
                    licence: "MIT".into(),
                },
                ModelNeed {
                    id: "b".into(),
                    label: "B".into(),
                    bytes: 50,
                    licence: "MIT".into(),
                },
            ]
        }
        fn state(&self, id: &str) -> NeedState {
            if id == "a" {
                NeedState::Ready
            } else {
                NeedState::Missing
            }
        }
        fn install(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn missing_bytes_ignores_ready_models() {
        assert_eq!(missing_bytes(&Fake), 50);
    }
}
