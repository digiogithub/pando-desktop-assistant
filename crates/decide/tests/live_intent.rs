//! Live intent accuracy against a real decision model (PDA-T-0098 acceptance):
//! 100 labelled phrases, > 90 % intent accuracy with `tev1:0.8b`.
//!
//! ```sh
//! ollama pull tev1:0.8b
//! DECIDE_BASE_URL=http://127.0.0.1:11434 cargo test -p decide --test live_intent -- --ignored --nocapture
//! ```

#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../src/bin/decide-eval/http.rs"]
mod http;

use decide::cascade::Cascade;
use decide::questions::Intent;
use decide::state::StateInput;
use decide::transport::{ClientOptions, SystemOneClient, block_on};

const PHRASES: &str = include_str!("fixtures/intent_phrases.tsv");

fn phrases() -> Vec<(Intent, &'static str)> {
    PHRASES
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (label, text) = l.split_once('\t').expect("label<TAB>text");
            (Intent::from_key(label).expect("known label"), text)
        })
        .collect()
}

#[test]
fn fixture_has_100_labelled_phrases() {
    let p = phrases();
    assert_eq!(p.len(), 100);
    for i in Intent::ALL {
        assert!(p.iter().any(|(l, _)| l == i), "{i:?} has no example");
    }
}

#[test]
#[ignore = "needs a running Ollama with tev1:0.8b"]
fn intent_accuracy_above_90_percent() {
    let base = std::env::var("DECIDE_BASE_URL").unwrap_or_else(|_| http::DEFAULT_BASE.into());
    let model = std::env::var("DECIDE_MODEL").unwrap_or_else(|_| "tev1:0.8b".into());
    let transport = http::StdHttpTransport::new(&base, None).expect("base url");
    let mut opts = ClientOptions::new(model);
    opts.timeout = http::EVAL_TIMEOUT;
    let client = SystemOneClient::new(transport, opts);

    let set = phrases();
    let mut ok = 0usize;
    for (label, text) in &set {
        let input = StateInput {
            transcript: (*text).into(),
            ..StateInput::default()
        };
        let o = client.options();
        let mut cascade = Cascade::new(
            input,
            o.model.clone(),
            o.keep_alive.clone(),
            o.context_tokens,
        );
        if cascade.intent().is_none() {
            let req = cascade.next_request().expect("intent request");
            let resp = block_on(client.decide(&req)).expect("decision call").value;
            cascade.feed(&resp).expect("valid intent answer");
        }
        let got = cascade.intent().map(|i| i.value);
        if got == Some(*label) {
            ok += 1;
        } else {
            println!("MISS {label:?} <- {text:?}: {got:?}");
        }
    }
    let acc = ok as f64 / set.len() as f64;
    println!("intent accuracy: {ok}/{} = {acc:.3}", set.len());
    assert!(acc > 0.90, "accuracy {acc:.3} <= 0.90");
}

#[test]
#[ignore = "needs a running Ollama with tev1:0.8b"]
fn log_and_replay_round_trip() {
    use decide::eval::replay;
    use decide::log::{DecisionLog, DecisionRecord, read_log};
    use decide::policy::{PolicyContext, Thresholds, policy};

    let base = std::env::var("DECIDE_BASE_URL").unwrap_or_else(|_| http::DEFAULT_BASE.into());
    let transport = http::StdHttpTransport::new(&base, None).expect("base url");
    let mut opts = ClientOptions::new("tev1:0.8b");
    opts.timeout = http::EVAL_TIMEOUT;
    let client = SystemOneClient::new(transport, opts);
    let dir = tempfile::tempdir().expect("tempdir");
    let log = DecisionLog::in_dir(dir.path());
    for (_, text) in phrases().iter().step_by(10) {
        let input = StateInput {
            transcript: (*text).into(),
            ..StateInput::default()
        };
        let result = block_on(decide::run_cascade(&client, input));
        let outcome = match &result {
            Ok(run) => policy(
                &run.decision,
                &PolicyContext {
                    transcript: text,
                    agents: &[],
                    pending: &[],
                    thresholds: Thresholds::default(),
                },
            ),
            Err(_) => decide::Outcome::unavailable(),
        };
        log.append(&DecisionRecord::new(0, *text, &result, outcome, false))
            .expect("append");
    }
    let (records, bad) = read_log(log.path()).expect("read");
    assert_eq!((records.len(), bad), (10, 0));
    let report = block_on(replay(&records, &client));
    println!("{}", report.render());
    // The same model on the same requests must agree with itself.
    for (name, q) in &report.questions {
        assert_eq!(q.agree, q.compared, "{name}");
    }
}
