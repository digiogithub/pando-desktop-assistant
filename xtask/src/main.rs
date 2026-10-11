//! `cargo xtask` entry point.

mod deps;

use std::process::ExitCode;

fn main() -> ExitCode {
    let cmd = std::env::args().nth(1);
    match cmd.as_deref() {
        Some("check-deps") => match deps::run() {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(e) => {
                eprintln!("check-deps: {e:#}");
                ExitCode::FAILURE
            }
        },
        Some(other) => {
            eprintln!("xtask: unknown command `{other}`");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("usage: cargo xtask <check-deps>");
            ExitCode::FAILURE
        }
    }
}
