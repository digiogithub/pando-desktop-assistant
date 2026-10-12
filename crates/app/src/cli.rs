//! Command-line arguments of the desktop binary.

use clap::Parser;

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

    /// Open the organism demo (fake agents, state selector) instead of the app; needs no Pando.
    #[arg(long)]
    pub demo: bool,
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
    }
}
