//! Command-line arguments of the desktop binary.

use clap::{Parser, Subcommand, ValueEnum};

/// Pando Desktop Assistant.
#[derive(Debug, Clone, Parser, PartialEq, Eq)]
#[command(name = "pando-assistant", version, about)]
pub struct Args {
    /// Default log filter (overridden by `RUST_LOG`), e.g. `debug` or `info,app=trace`.
    #[arg(long, value_name = "LEVEL")]
    pub log_level: Option<String>,

    /// Open the window, wait for two frames, log the GPU specs and exit with 0. Uses a
    /// throwaway data root so it never touches the user's files; a 30 s watchdog exits with 2.
    #[arg(long)]
    pub smoke_test: bool,

    /// Start hidden in the tray (what autostart passes); falls back to the window when no tray
    /// registers.
    #[arg(long)]
    pub minimized: bool,

    /// Open the organism demo (fake agents, state selector) instead of the app; needs no Pando.
    #[arg(long)]
    pub demo: bool,

    /// Control a running instance instead of starting one.
    #[command(subcommand)]
    pub command: Option<Cmd>,
}

/// Which push-to-talk edge `ptt` sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PttArg {
    /// Start listening (bind to the key press).
    Start,
    /// Stop listening and send (bind to the key release).
    Stop,
    /// Flip.
    Toggle,
}

/// Commands for the running instance (they exit with 1 when none is running).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum Cmd {
    /// Push-to-talk, for compositor bindings (Sway: `bindsym --no-repeat $mod+space exec
    /// pando-assistant ptt start` and `bindsym --release $mod+space exec pando-assistant ptt stop`).
    Ptt {
        /// Edge to send.
        #[arg(value_enum)]
        action: PttArg,
    },
    /// Show and focus the main window.
    Show,
    /// Hide the main window.
    Hide,
    /// Show / hide the floating mini window.
    ToggleMini,
    /// Open the settings window.
    Settings,
    /// Open the About window (licences, source offer).
    About,
    /// Quit the running instance.
    Quit,
}

impl Cmd {
    /// The IPC command this subcommand sends.
    #[must_use]
    pub fn to_ipc(self) -> crate::shell::ipc::Command {
        use crate::shell::ipc::{Command, PttAction};
        match self {
            Self::Ptt { action } => Command::Ptt {
                action: match action {
                    PttArg::Start => PttAction::Start,
                    PttArg::Stop => PttAction::Stop,
                    PttArg::Toggle => PttAction::Toggle,
                },
            },
            Self::Show => Command::Show,
            Self::Hide => Command::Hide,
            Self::ToggleMini => Command::ToggleMini,
            Self::Settings => Command::OpenSettings,
            Self::About => Command::OpenAbout,
            Self::Quit => Command::Quit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_parse() {
        let args = Args::parse_from(["pando-assistant", "--smoke-test", "--log-level", "debug"]);
        assert!(args.smoke_test);
        assert_eq!(args.log_level.as_deref(), Some("debug"));
        assert!(!Args::parse_from(["pando-assistant"]).smoke_test);
        assert!(Args::parse_from(["pando-assistant", "--demo"]).demo);
        let a = Args::parse_from(["pando-assistant", "ptt", "start"]);
        assert_eq!(
            a.command.map(Cmd::to_ipc),
            Some(crate::shell::ipc::Command::Ptt {
                action: crate::shell::ipc::PttAction::Start
            })
        );
        assert!(Args::try_parse_from(["pando-assistant", "ptt", "bogus"]).is_err());
    }
}
