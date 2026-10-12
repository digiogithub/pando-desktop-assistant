//! The shell controller: one inbox for every trigger (IPC, hotkeys, tray, portal) and the pure
//! state machine that turns commands into [`Effect`]s. GPUI performs the effects
//! (`super::runtime`), the voice layer reads the [`Output`]s.

use super::ipc::{Command, PttAction};
use super::ptt::{PttEdge, PttInput, PttMachine};

/// Everything that can arrive in the shell inbox.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Msg {
    /// A command from IPC, a hotkey or the tray.
    Command(Command),
    /// A downloaded update waits for a restart.
    UpdateReady {
        /// Version string.
        version: String,
    },
    /// The tray icon registered (`true`) or failed / went away (`false`).
    Tray(bool),
    /// A global-shortcut backend reported its status (for settings / logs).
    Backend {
        /// Backend name (`global-hotkey`, `portal`).
        name: &'static str,
        /// Human-readable state.
        status: String,
    },
}

impl From<Command> for Msg {
    fn from(c: Command) -> Self {
        Self::Command(c)
    }
}

/// What the rest of the app must do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Show (re-opening if needed) and focus the main window.
    ShowMain,
    /// Hide the main window (close to tray when a tray exists, else minimize).
    HideMain,
    /// Show / hide the mini window.
    ToggleMini,
    /// Show the main window and select a cell.
    FocusCell(String),
    /// Open the settings window.
    OpenSettings,
    /// Open the first-run assistant.
    OpenFirstRun,
    /// Open the About window.
    OpenAbout,
    /// Quit the process.
    Quit,
    /// Something for the voice orchestrator.
    Output(Output),
}

/// Signals for the voice layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// Push-to-talk edge.
    Ptt(PttEdge),
    /// Hands-free listening changed.
    HandsFree(bool),
    /// Assistant voice muted / unmuted.
    Muted(bool),
}

/// Shell state: what the tray menu shows and what PTT triggers have done.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellState {
    /// Push-to-talk machine.
    pub ptt: PttMachine,
    /// Hands-free listening.
    pub hands_free: bool,
    /// Voice muted.
    pub muted: bool,
    /// A tray icon exists, so closing the window can keep the app alive.
    pub tray: bool,
    /// The user wants the app to keep running when the window closes.
    pub keep_running: bool,
}

impl ShellState {
    /// Starting state from the settings.
    #[must_use]
    pub fn new(keep_running: bool) -> Self {
        Self {
            keep_running,
            ..Self::default()
        }
    }

    /// Closing the main window keeps the process alive.
    #[must_use]
    pub fn background_mode(&self) -> bool {
        self.keep_running && self.tray
    }

    /// Applies one message.
    pub fn apply(&mut self, msg: &Msg) -> Vec<Effect> {
        match msg {
            Msg::Tray(on) => {
                self.tray = *on;
                Vec::new()
            }
            Msg::Backend { .. } | Msg::UpdateReady { .. } => Vec::new(),
            Msg::Command(c) => self.command(c),
        }
    }

    fn command(&mut self, c: &Command) -> Vec<Effect> {
        match c {
            Command::Ptt { action } => {
                let input = match action {
                    PttAction::Start => PttInput::Pressed,
                    PttAction::Stop => PttInput::Released,
                    PttAction::Toggle => PttInput::Toggle,
                };
                self.ptt
                    .apply(input)
                    .map(|edge| vec![Effect::Output(Output::Ptt(edge))])
                    .unwrap_or_default()
            }
            Command::Focus | Command::Show => vec![Effect::ShowMain],
            Command::FocusCell { id } => vec![Effect::FocusCell(id.clone())],
            Command::Hide => vec![Effect::HideMain],
            Command::ToggleMini => vec![Effect::ToggleMini],
            Command::OpenSettings => vec![Effect::OpenSettings],
            Command::OpenFirstRun => vec![Effect::OpenFirstRun],
            Command::OpenAbout => vec![Effect::OpenAbout],
            Command::Quit => vec![Effect::Quit],
            Command::ToggleHandsFree => {
                self.hands_free = !self.hands_free;
                vec![Effect::Output(Output::HandsFree(self.hands_free))]
            }
            Command::ToggleMute => {
                self.muted = !self.muted;
                vec![Effect::Output(Output::Muted(self.muted))]
            }
            Command::Ping => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(c: Command) -> Msg {
        Msg::Command(c)
    }

    #[test]
    fn ptt_commands_only_emit_real_edges() {
        let mut s = ShellState::new(true);
        let start = cmd(Command::Ptt {
            action: PttAction::Start,
        });
        assert_eq!(
            s.apply(&start),
            vec![Effect::Output(Output::Ptt(PttEdge::Started))]
        );
        assert!(s.apply(&start).is_empty(), "auto-repeat");
        assert_eq!(
            s.apply(&cmd(Command::Ptt {
                action: PttAction::Stop
            })),
            vec![Effect::Output(Output::Ptt(PttEdge::Stopped))]
        );
    }

    #[test]
    fn window_commands_map_to_effects_and_second_launch_focuses() {
        let mut s = ShellState::new(true);
        assert_eq!(s.apply(&cmd(Command::Focus)), vec![Effect::ShowMain]);
        assert_eq!(s.apply(&cmd(Command::Hide)), vec![Effect::HideMain]);
        assert_eq!(s.apply(&cmd(Command::ToggleMini)), vec![Effect::ToggleMini]);
        assert_eq!(s.apply(&cmd(Command::Quit)), vec![Effect::Quit]);
        assert!(s.apply(&cmd(Command::Ping)).is_empty());
    }

    #[test]
    fn background_mode_needs_the_setting_and_a_tray() {
        let mut s = ShellState::new(true);
        assert!(!s.background_mode());
        s.apply(&Msg::Tray(true));
        assert!(s.background_mode());
        s.keep_running = false;
        assert!(!s.background_mode());
    }

    #[test]
    fn tray_toggles_flip_and_report() {
        let mut s = ShellState::new(true);
        assert_eq!(
            s.apply(&cmd(Command::ToggleHandsFree)),
            vec![Effect::Output(Output::HandsFree(true))]
        );
        assert_eq!(
            s.apply(&cmd(Command::ToggleMute)),
            vec![Effect::Output(Output::Muted(true))]
        );
        assert_eq!(
            s.apply(&cmd(Command::ToggleMute)),
            vec![Effect::Output(Output::Muted(false))]
        );
    }
}
