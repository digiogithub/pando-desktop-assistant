//! `cargo xtask` entry point.

mod deps;
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
        Some("tokens") => report("tokens", tokens::run(&args[1..])),
        Some(other) => {
            eprintln!("xtask: unknown command `{other}`");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("usage: cargo xtask <check-deps|tokens [--check|--contrast]>");
            ExitCode::FAILURE
        }
    }
}
