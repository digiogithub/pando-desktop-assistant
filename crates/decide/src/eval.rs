//! Model evaluation over `decisions.jsonl` (`PDA-SP-0008.R5`).
//!
//! [`replay`] re-sends every logged request (same state and questions) to another
//! model and compares the answers question by question: agreement of the choice, and
//! the deltas of `p(top)` and confidence (candidate − baseline). [`compare_logs`] does
//! the same offline between two logs of the same transcripts. The report is
//! deterministic for a deterministic model (no timings in it), so it can be diffed.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::log::DecisionRecord;
use crate::protocol::{Answer, Response};
use crate::transport::{DecisionTransport, SystemOneClient};

/// Per-question statistics.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QuestionStats {
    /// Answers compared.
    pub compared: usize,
    /// Same choice.
    pub agree: usize,
    /// Sum of `p(top)` deltas (candidate − baseline) where both are valid.
    sum_dp: f64,
    n_dp: usize,
    /// Sum of confidence deltas where both are present.
    sum_dconf: f64,
    n_dconf: usize,
}

impl QuestionStats {
    fn add(&mut self, base: &Answer, cand: &Answer) {
        self.compared += 1;
        if base.choice.is_some() && base.choice == cand.choice {
            self.agree += 1;
        }
        if let (Some(b), Some(c)) = (base.p_top(), cand.p_top()) {
            self.sum_dp += c - b;
            self.n_dp += 1;
        }
        if let (Some(b), Some(c)) = (base.confidence, cand.confidence)
            && b.is_finite()
            && c.is_finite()
        {
            self.sum_dconf += c - b;
            self.n_dconf += 1;
        }
    }

    /// Agreement rate in `[0, 1]` (0 when nothing was compared).
    pub fn agreement(&self) -> f64 {
        if self.compared == 0 {
            0.0
        } else {
            self.agree as f64 / self.compared as f64
        }
    }

    /// Mean `p(top)` delta.
    pub fn mean_dp(&self) -> Option<f64> {
        (self.n_dp > 0).then(|| self.sum_dp / self.n_dp as f64)
    }

    /// Mean confidence delta.
    pub fn mean_dconf(&self) -> Option<f64> {
        (self.n_dconf > 0).then(|| self.sum_dconf / self.n_dconf as f64)
    }
}

/// One disagreement, for the report's detail section.
#[derive(Debug, Clone, PartialEq)]
pub struct Disagreement {
    /// Transcript.
    pub transcript: String,
    /// Question name.
    pub question: String,
    /// Baseline choice.
    pub baseline: String,
    /// Candidate choice.
    pub candidate: String,
}

/// Evaluation report.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EvalReport {
    /// Baseline model(s) as found in the log.
    pub baseline: String,
    /// Candidate model.
    pub candidate: String,
    /// Records read.
    pub records: usize,
    /// Requests compared.
    pub requests: usize,
    /// Per question name.
    pub questions: BTreeMap<String, QuestionStats>,
    /// Candidate errors by kind (replay) or unmatched records (offline).
    pub errors: BTreeMap<String, usize>,
    /// Every disagreement, in log order.
    pub disagreements: Vec<Disagreement>,
}

impl EvalReport {
    fn compare(&mut self, transcript: &str, base: &Response, cand: &Response) {
        self.requests += 1;
        for (name, b) in &base.answers {
            let Some(c) = cand.answers.get(name) else {
                *self.errors.entry("missing_answer".into()).or_default() += 1;
                continue;
            };
            self.questions.entry(name.clone()).or_default().add(b, c);
            if b.choice != c.choice {
                self.disagreements.push(Disagreement {
                    transcript: transcript.to_string(),
                    question: name.clone(),
                    baseline: b.choice.clone().unwrap_or_default(),
                    candidate: c.choice.clone().unwrap_or_default(),
                });
            }
        }
    }

    /// Plain-text report (developer tool output, not UI copy).
    pub fn render(&self) -> String {
        let f = |v: Option<f64>| v.map_or_else(|| "n/a".to_string(), |x| format!("{x:+.3}"));
        let mut s = String::new();
        let _ = writeln!(s, "decision eval: {} -> {}", self.baseline, self.candidate);
        let _ = writeln!(s, "records: {}  requests: {}", self.records, self.requests);
        let _ = writeln!(
            s,
            "{:<10} {:>8} {:>10} {:>9} {:>11}",
            "question", "compared", "agreement", "mean dp", "mean dconf"
        );
        for (name, q) in &self.questions {
            let _ = writeln!(
                s,
                "{:<10} {:>8} {:>10.3} {:>9} {:>11}",
                name,
                q.compared,
                q.agreement(),
                f(q.mean_dp()),
                f(q.mean_dconf())
            );
        }
        if !self.errors.is_empty() {
            let _ = writeln!(s, "errors:");
            for (k, n) in &self.errors {
                let _ = writeln!(s, "  {k}: {n}");
            }
        }
        if !self.disagreements.is_empty() {
            let _ = writeln!(s, "disagreements:");
            for d in &self.disagreements {
                let _ = writeln!(
                    s,
                    "  [{}] {:?}: {} -> {}",
                    d.question, d.transcript, d.baseline, d.candidate
                );
            }
        }
        s
    }
}

fn baseline_models(records: &[DecisionRecord]) -> String {
    let mut m: Vec<&str> = records
        .iter()
        .map(|r| r.model.as_str())
        .filter(|m| !m.is_empty())
        .collect();
    m.sort_unstable();
    m.dedup();
    m.join(",")
}

/// Replay every logged request against the client's model (same state and questions,
/// model replaced) and compare with the logged answers. Candidate failures are counted
/// by error kind and skipped.
pub async fn replay<T: DecisionTransport>(
    records: &[DecisionRecord],
    client: &SystemOneClient<T>,
) -> EvalReport {
    let mut report = EvalReport {
        baseline: baseline_models(records),
        candidate: client.options().model.clone(),
        records: records.len(),
        ..EvalReport::default()
    };
    for r in records {
        for ex in &r.exchanges {
            let mut req = ex.request.clone();
            req.model = client.options().model.clone();
            req.keep_alive = client.options().keep_alive.clone();
            match client.decide(&req).await {
                Ok(t) => report.compare(&r.transcript, &ex.response, &t.value),
                Err(e) => *report.errors.entry(e.kind().to_string()).or_default() += 1,
            }
        }
    }
    report
}

/// Offline comparison of two logs: records are paired in order by transcript, and the
/// exchanges of a pair are compared by position while their question sets match.
pub fn compare_logs(baseline: &[DecisionRecord], candidate: &[DecisionRecord]) -> EvalReport {
    let mut report = EvalReport {
        baseline: baseline_models(baseline),
        candidate: baseline_models(candidate),
        records: baseline.len(),
        ..EvalReport::default()
    };
    let mut pool: BTreeMap<&str, std::collections::VecDeque<&DecisionRecord>> = BTreeMap::new();
    for c in candidate {
        pool.entry(c.transcript.as_str()).or_default().push_back(c);
    }
    for b in baseline {
        let Some(c) = pool
            .get_mut(b.transcript.as_str())
            .and_then(|q| q.pop_front())
        else {
            *report.errors.entry("unmatched_record".into()).or_default() += 1;
            continue;
        };
        for (be, ce) in b.exchanges.iter().zip(&c.exchanges) {
            let same = be.request.questions.len() == ce.request.questions.len()
                && be
                    .request
                    .questions
                    .iter()
                    .zip(&ce.request.questions)
                    .all(|((a, _), (b, _))| a == b);
            if !same {
                *report.errors.entry("diverged_cascade".into()).or_default() += 1;
                break;
            }
            report.compare(&b.transcript, &be.response, &ce.response);
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cascade::run;
    use crate::policy::Outcome;
    use crate::state::StateInput;
    use crate::transport::mock::{MockTransport, Reply};
    use crate::transport::{ClientOptions, TransportError, block_on};

    fn record(transcript: &str, replies: Vec<Reply>) -> DecisionRecord {
        let c = SystemOneClient::new(MockTransport::new(replies), ClientOptions::new("tev1:0.8b"));
        let result = block_on(run(
            &c,
            StateInput {
                transcript: transcript.into(),
                ..StateInput::default()
            },
        ));
        DecisionRecord::new(0, transcript, &result, Outcome::unavailable(), false)
    }

    fn sample_log() -> Vec<DecisionRecord> {
        vec![
            record(
                "explícame el error",
                vec![Reply::Answer(vec![("intent", "question", 0.9)])],
            ),
            record(
                "cancela migración auth",
                vec![
                    Reply::Answer(vec![("intent", "stop_agent", 0.8)]),
                    Reply::Answer(vec![("agent", "none", 0.6), ("action", "cancel", 0.7)]),
                ],
            ),
        ]
    }

    #[test]
    fn replay_report_is_reproducible() {
        let log = sample_log();
        let candidate = || {
            SystemOneClient::new(
                MockTransport::new(vec![
                    Reply::Answer(vec![("intent", "work_request", 0.7)]),
                    Reply::Answer(vec![("intent", "stop_agent", 0.9)]),
                    Reply::Fail(TransportError::Timeout),
                ]),
                ClientOptions::new("jev-latest"),
            )
        };
        let r1 = block_on(replay(&log, &candidate()));
        let r2 = block_on(replay(&log, &candidate()));
        assert_eq!(r1.render(), r2.render());
        assert_eq!(r1.requests, 2);
        let intent = &r1.questions["intent"];
        assert_eq!((intent.compared, intent.agree), (2, 1));
        assert_eq!(r1.errors["timeout"], 1);
        let expected = "\
decision eval: tev1:0.8b -> jev-latest
records: 2  requests: 2
question   compared  agreement   mean dp  mean dconf
intent            2      0.500    -0.050      -0.050
errors:
  timeout: 1
disagreements:
  [intent] \"explícame el error\": question -> work_request
";
        assert_eq!(r1.render(), expected);
    }

    #[test]
    fn replay_sends_the_candidate_model_with_the_logged_state() {
        let log = sample_log();
        let c = SystemOneClient::new(
            MockTransport::new(vec![Reply::Answer(vec![("intent", "question", 0.9)])]),
            ClientOptions::new("jev-latest"),
        );
        let _ = block_on(replay(&log[..1], &c));
        let seen = c.transport().seen_json();
        assert_eq!(seen[0]["model"], "jev-latest");
        assert_eq!(
            seen[0]["state"].as_str(),
            Some(log[0].exchanges[0].request.state.as_str())
        );
    }

    #[test]
    fn offline_comparison_pairs_by_transcript() {
        let base = sample_log();
        let cand = vec![
            record(
                "cancela migración auth",
                vec![
                    Reply::Answer(vec![("intent", "stop_agent", 0.9)]),
                    Reply::Answer(vec![("agent", "none", 0.6), ("action", "pause", 0.5)]),
                ],
            ),
            record(
                "otra cosa",
                vec![Reply::Answer(vec![("intent", "chat", 0.9)])],
            ),
        ];
        let r = compare_logs(&base, &cand);
        assert_eq!(r.errors["unmatched_record"], 1);
        assert_eq!(r.requests, 2);
        assert_eq!(r.questions["action"].agree, 0);
        assert_eq!(r.questions["agent"].agree, 1);
        assert!((r.questions["intent"].mean_dp().unwrap() - 0.1).abs() < 1e-9);
        assert_eq!(r.disagreements.len(), 1);
    }
}
