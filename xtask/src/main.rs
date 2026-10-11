//! `cargo xtask` entry point (skeleton).

use std::process::ExitCode;

fn main() -> ExitCode {
    let cmd = std::env::args().nth(1);
    match cmd.as_deref() {
        Some(other) => {
            eprintln!("xtask: unknown command `{other}`");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("usage: cargo xtask <command>");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert_eq!(2 + 2, 4);
    }
}
