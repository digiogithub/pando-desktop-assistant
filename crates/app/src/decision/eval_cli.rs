//! `pando-assistant decisions eval|compare` (PDA-T-0106): replays a `decisions.jsonl` against
//! another decision model and prints the deterministic agreement report. Handled before the
//! GUI argument parser so the GUI flags stay untouched; it never opens a window.

use std::path::Path;
use std::time::Duration;

use decide::eval::{compare_logs, replay};
use decide::log::{DecisionRecord, read_log};
use decide::transport::{ClientOptions, SystemOneClient};

use super::transport::{DecisionEndpoint, HttpDecisionTransport, KEY_ENV_VARS, Secret};

const USAGE: &str = "usage:
  pando-assistant decisions eval [<decisions.jsonl>] --model <m> [--base-url URL] [--timeout-ms N] [--api-key-env VAR]
  pando-assistant decisions compare <baseline.jsonl> <candidate.jsonl>
(the default log is <data dir>/decisions.jsonl; the default base URL is http://127.0.0.1:11434)";

/// Default evaluation base URL (a local Ollama).
pub const DEFAULT_BASE: &str = "http://127.0.0.1:11434";

fn load(path: &Path) -> Result<Vec<DecisionRecord>, String> {
    let (records, bad) = read_log(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bad > 0 {
        eprintln!(
            "decisions: skipped {bad} malformed line(s) in {}",
            path.display()
        );
    }
    Ok(records)
}

/// True when `args` (without the program name) start with `decisions`.
#[must_use]
pub fn handles(args: &[String]) -> bool {
    args.first().is_some_and(|a| a == "decisions")
}

/// Runs the subcommand and returns the report to print.
pub fn run(args: &[String], default_log: &Path) -> Result<String, String> {
    match args.get(1).map(String::as_str) {
        Some("eval") => eval(&args[2..], default_log),
        Some("compare") => {
            let (a, b) = (args.get(2).ok_or(USAGE)?, args.get(3).ok_or(USAGE)?);
            Ok(compare_logs(&load(Path::new(a))?, &load(Path::new(b))?).render())
        }
        _ => Err(USAGE.to_owned()),
    }
}

#[allow(clippy::disallowed_methods)] // a CLI: no GUI thread to block
fn eval(args: &[String], default_log: &Path) -> Result<String, String> {
    let mut file = None;
    let (mut model, mut base, mut timeout, mut key_env) =
        (None, DEFAULT_BASE.to_owned(), Duration::from_secs(30), None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(flag) = a.strip_prefix("--") {
            let value = it.next().ok_or_else(|| format!("--{flag} needs a value"))?;
            match flag {
                "model" => model = Some(value.clone()),
                "base-url" => base = value.clone(),
                "timeout-ms" => {
                    timeout = Duration::from_millis(
                        value.parse().map_err(|_| "bad --timeout-ms".to_owned())?,
                    );
                }
                "api-key-env" => key_env = Some(value.clone()),
                other => return Err(format!("unknown flag --{other}\n{USAGE}")),
            }
        } else {
            file = Some(a.clone());
        }
    }
    let model = model.ok_or_else(|| format!("--model is required\n{USAGE}"))?;
    let path = file.map_or_else(|| default_log.to_path_buf(), Into::into);
    let key = key_env
        .iter()
        .map(String::as_str)
        .chain(KEY_ENV_VARS)
        .find_map(|v| std::env::var(v).ok())
        .filter(|k| !k.trim().is_empty());
    let endpoint = DecisionEndpoint {
        url: format!("{}/v1/systemone", base.trim_end_matches('/')),
        model: model.clone(),
        key: key.map(Secret::new),
        local: true,
    };
    let transport = HttpDecisionTransport::new(endpoint).map_err(|e| e.to_string())?;
    let mut options = ClientOptions::new(model);
    options.timeout = timeout;
    let client = SystemOneClient::new(transport, options);
    let records = load(&path)?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(rt.block_on(replay(&records, &client)).render())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn only_the_decisions_subcommand_is_handled() {
        assert!(handles(&s(&["decisions", "eval"])));
        assert!(!handles(&s(&["--smoke-test"])));
        assert!(!handles(&[]));
    }

    #[test]
    fn usage_errors_do_not_panic() {
        let log = Path::new("/nonexistent/decisions.jsonl");
        assert!(run(&s(&["decisions"]), log).is_err());
        assert!(
            run(&s(&["decisions", "eval"]), log)
                .expect_err("model")
                .contains("--model")
        );
        assert!(run(&s(&["decisions", "eval", "--bogus", "x"]), log).is_err());
        assert!(run(&s(&["decisions", "compare", "a"]), log).is_err());
    }

    #[test]
    fn compare_is_deterministic_over_a_sample_log() {
        let dir = tempfile::tempdir().expect("tmp");
        let line = r#"{"ts_ms":1,"transcript":"hola","state_hash":"","model":"a","exchanges":[],"latency_ms":0,"policy":{"outcome":"conversation","reason":"intent","top2":[]},"executed":false}"#;
        let a = dir.path().join("a.jsonl");
        std::fs::write(&a, format!("{line}\n")).expect("w");
        let r1 = run(
            &s(&[
                "decisions",
                "compare",
                a.to_str().expect("p"),
                a.to_str().expect("p"),
            ]),
            &a,
        )
        .expect("ok");
        let r2 = run(
            &s(&[
                "decisions",
                "compare",
                a.to_str().expect("p"),
                a.to_str().expect("p"),
            ]),
            &a,
        )
        .expect("ok");
        assert_eq!(r1, r2);
        assert!(!r1.is_empty());
    }
}
