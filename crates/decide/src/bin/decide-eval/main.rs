//! `decide-eval`: developer CLI to compare decision models over `decisions.jsonl`.
//!
//! ```text
//! decide-eval eval <decisions.jsonl> --model <m> [--base-url http://127.0.0.1:11434]
//!                  [--timeout-ms 30000] [--api-key-env VAR]
//! decide-eval compare <baseline.jsonl> <candidate.jsonl>
//! ```
//!
//! It lives in `decide` (not `xtask`, which may not depend on workspace crates, and not
//! `app`, which pulls GPUI) so it can be built and run headless. Its transport is plain
//! `std::net` HTTP (a local Ollama); the library itself stays free of network code.
//! The user-facing `pando-assistant decisions eval` subcommand wraps the same
//! `decide::eval` functions with the app's TLS-capable transport.

mod http;

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use decide::eval::{compare_logs, replay};
use decide::log::read_log;
use decide::transport::{ClientOptions, SystemOneClient, block_on};

const USAGE: &str = "usage:
  decide-eval eval <decisions.jsonl> --model <m> [--base-url URL] [--timeout-ms N] [--api-key-env VAR]
  decide-eval compare <baseline.jsonl> <candidate.jsonl>";

fn load(path: &str) -> Result<Vec<decide::log::DecisionRecord>, String> {
    let (records, bad) = read_log(Path::new(path)).map_err(|e| format!("{path}: {e}"))?;
    if bad > 0 {
        eprintln!("decide-eval: skipped {bad} malformed line(s) in {path}");
    }
    Ok(records)
}

fn run(args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("eval") => {
            let file = args.get(1).ok_or(USAGE)?;
            let mut model = None;
            let mut base = http::DEFAULT_BASE.to_string();
            let mut timeout = http::EVAL_TIMEOUT;
            let mut key_env = None;
            let mut it = args[2..].iter();
            while let Some(flag) = it.next() {
                let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
                match flag.as_str() {
                    "--model" => model = Some(value.clone()),
                    "--base-url" => base = value.clone(),
                    "--timeout-ms" => {
                        timeout = Duration::from_millis(
                            value.parse().map_err(|_| "bad --timeout-ms".to_string())?,
                        );
                    }
                    "--api-key-env" => key_env = Some(value.clone()),
                    other => return Err(format!("unknown flag {other}\n{USAGE}")),
                }
            }
            let model = model.ok_or_else(|| format!("--model is required\n{USAGE}"))?;
            let key = key_env.and_then(|v| std::env::var(v).ok());
            let transport = http::StdHttpTransport::new(&base, key)?;
            let mut opts = ClientOptions::new(model);
            opts.timeout = timeout;
            let client = SystemOneClient::new(transport, opts);
            let records = load(file)?;
            Ok(block_on(replay(&records, &client)).render())
        }
        Some("compare") => {
            let (a, b) = (args.get(1).ok_or(USAGE)?, args.get(2).ok_or(USAGE)?);
            Ok(compare_logs(&load(a)?, &load(b)?).render())
        }
        _ => Err(USAGE.to_string()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("decide-eval: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_errors() {
        assert!(run(&[]).is_err());
        assert!(run(&["eval".into()]).is_err());
        assert!(run(&["eval".into(), "f".into(), "--model".into()]).is_err());
        assert!(run(&["eval".into(), "f".into(), "--wat".into(), "x".into()]).is_err());
    }

    #[test]
    fn parses_chunked_and_identity_responses() {
        let r = http::parse_response(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
        )
        .unwrap();
        assert_eq!((r.status, r.body.as_slice()), (200, &b"hello world"[..]));
        let r =
            http::parse_response(b"HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\n\r\n{}").unwrap();
        assert_eq!((r.status, r.body.as_slice()), (404, &b"{}"[..]));
        assert!(http::parse_response(b"garbage").is_err());
        assert!(http::StdHttpTransport::new("https://x", None).is_err());
    }
}
