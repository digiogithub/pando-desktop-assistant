//! `cargo xtask` entry point.

mod deps;
mod release;
mod tokens;
mod tokens_organism;

use std::process::ExitCode;

fn report(name: &str, result: anyhow::Result<bool>) -> ExitCode {
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{name}: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check-deps") => report("check-deps", deps::run()),
        Some("notice") => report("notice", release::notice()),
        Some("check-nm") => report("check-nm", release::check_nm(&args[1..])),
        Some("source-bundle") => report("source-bundle", release::source_bundle()),
        Some("bundle") => report("bundle", release::bundle(&args[1..])),
        Some("tokens") => report("tokens", tokens::run(&args[1..])),
        Some(other) => {
            eprintln!("xtask: unknown command `{other}`");
            ExitCode::FAILURE
        }
        None => {
            eprintln!(
                "usage: cargo xtask <check-deps|tokens [--check|--contrast]|notice|check-nm|source-bundle|bundle>"
            );
            ExitCode::FAILURE
        }
    }
}
