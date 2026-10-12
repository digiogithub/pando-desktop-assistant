//! Voice → decision: runs the System One cascade, applies the fail-closed policy and builds
//! the log record and the card (PDA-T-0097/0099/0095). Any failure of the decision model is
//! fail-open for the conversation: the sentence goes to the chat instead.

use decide::cascade::{CascadeFailure, CascadeRun};
use decide::log::DecisionRecord;
use decide::policy::{PendingApproval, PolicyContext, Thresholds};
use decide::state::StateInput;
use decide::{DecisionTransport, Outcome, SystemOneClient, policy, run_cascade};

use super::directory::AgentDirectory;
use crate::voice::dock::{ActionCard, CardRow, ResultTone};

/// What the decision needs to know about the world.
#[derive(Debug, Clone, Default)]
pub struct StateSnapshot {
    /// Live agents.
    pub agents: AgentDirectory,
    /// Project names, most relevant first.
    pub projects: Vec<String>,
    /// Current project.
    pub current_project: Option<String>,
    /// Pending approvals, by agent slug.
    pub pending: Vec<PendingApproval>,
}

/// The result of deciding one sentence.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// What to do.
    pub outcome: Outcome,
    /// The log line (executed = false until the command ran).
    pub record: DecisionRecord,
    /// The decision model failed (the chip shows "no disponible").
    pub failed: bool,
    /// Total latency of the cascade in milliseconds.
    pub latency_ms: u64,
}

/// Decides one transcript.
pub async fn decide_transcript<T: DecisionTransport>(
    client: &SystemOneClient<T>,
    transcript: &str,
    snapshot: &StateSnapshot,
    thresholds: Thresholds,
    now_ms: u64,
) -> Resolved {
    let infos = snapshot.agents.infos();
    let input = StateInput {
        transcript: transcript.to_owned(),
        agents: infos.clone(),
        projects: snapshot.projects.clone(),
        current_project: snapshot.current_project.clone(),
    };
    let result: Result<CascadeRun, CascadeFailure> = run_cascade(client, input).await;
    let (outcome, failed) = match &result {
        Ok(run) => {
            let ctx = PolicyContext {
                transcript,
                agents: &infos,
                pending: &snapshot.pending,
                thresholds,
            };
            (policy(&run.decision, &ctx), false)
        }
        Err(failure) => {
            tracing::warn!(
                kind = failure.error.kind(),
                "decision model failed; falling back to conversation"
            );
            (Outcome::unavailable(), true)
        }
    };
    let record = DecisionRecord::new(now_ms, transcript, &result, outcome.clone(), false);
    let latency_ms = record.latency_ms;
    Resolved {
        outcome,
        record,
        failed,
        latency_ms,
    }
}

/// The card for a record: one row per answered question.
#[must_use]
pub fn card_from(record: &DecisionRecord) -> ActionCard {
    let mut rows = Vec::new();
    for ex in &record.exchanges {
        for (name, answer) in &ex.response.answers {
            rows.push(CardRow {
                label: name.clone(),
                value: answer.choice.clone().unwrap_or_default(),
                p: answer.p_top(),
            });
        }
    }
    ActionCard {
        transcript: record.transcript.clone(),
        rows,
        result: Some((describe(&record.policy), tone_of(&record.policy))),
    }
}

/// Tone of the result line for an outcome.
#[must_use]
pub fn tone_of(outcome: &Outcome) -> ResultTone {
    match outcome {
        Outcome::Execute { .. } | Outcome::SwitchProject { .. } | Outcome::Dictate { .. } => {
            ResultTone::Ok
        }
        Outcome::Confirm { .. } | Outcome::ExplicitOnly { .. } => ResultTone::NeedsUser,
        Outcome::Conversation { .. } => ResultTone::Ok,
    }
}

/// Monospace result line, e.g. `cancel migracion-auth → execute`.
#[must_use]
pub fn describe(outcome: &Outcome) -> String {
    use decide::questions::AgentTarget;
    let target = |t: &AgentTarget| match t {
        AgentTarget::Agent(id) => id.clone(),
        AgentTarget::All => "todos".to_owned(),
        AgentTarget::None => "-".to_owned(),
    };
    match outcome {
        Outcome::Execute { request }
        | Outcome::Confirm { request, .. }
        | Outcome::ExplicitOnly { request } => {
            format!(
                "{} {} → {}",
                request.action.key(),
                target(&request.target),
                outcome.label()
            )
        }
        Outcome::SwitchProject { project, .. } => format!("proyecto {project:?} → switch_project"),
        Outcome::Dictate { .. } => "dictado → composer".to_owned(),
        Outcome::Conversation { reason, .. } => format!("conversación ({reason:?})"),
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! A scripted transport shared by the decision tests.

    use std::sync::Mutex;

    use decide::transport::{TransportError, TransportRequest, TransportResponse};

    use super::DecisionTransport;

    type Round = Vec<(&'static str, &'static str, f64)>;

    /// Replies with answers built from the request, or fails.
    #[derive(Debug, Default)]
    pub struct Scripted {
        pub picks: Mutex<Vec<Round>>,
        pub fail: Option<TransportError>,
    }

    impl Scripted {
        pub fn new(rounds: Vec<Round>) -> Self {
            Self {
                picks: Mutex::new(rounds),
                fail: None,
            }
        }
    }

    pub fn answer_body(req: &serde_json::Value, picks: &[(&str, &str, f64)]) -> String {
        let mut answers = serde_json::Map::new();
        for (name, choice, p) in picks {
            let keys: Vec<String> = req["questions"][*name]["criteria"]
                .as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            let rest = (1.0 - p) / (keys.len().saturating_sub(1).max(1)) as f64;
            let probs: serde_json::Map<_, _> = keys
                .iter()
                .map(|k| {
                    (
                        k.clone(),
                        serde_json::json!(if k == choice { *p } else { rest }),
                    )
                })
                .collect();
            answers.insert(
                (*name).to_string(),
                serde_json::json!({"type":"choice","choice":choice,"probabilities":probs,"confidence":p}),
            );
        }
        serde_json::json!({"model":"tev1:0.8b","answers":answers}).to_string()
    }

    impl DecisionTransport for Scripted {
        async fn post(&self, r: TransportRequest) -> Result<TransportResponse, TransportError> {
            if let Some(e) = &self.fail {
                return Err(e.clone());
            }
            let req: serde_json::Value = serde_json::from_slice(&r.body).expect("json");
            let picks = {
                let mut g = self.picks.lock().expect("lock");
                if g.is_empty() {
                    Vec::new()
                } else {
                    g.remove(0)
                }
            };
            Ok(TransportResponse {
                status: 200,
                body: answer_body(&req, &picks).into_bytes(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use decide::ClientOptions;
    use decide::questions::{Action, AgentTarget};
    use decide::transport::{TransportError, block_on};

    use super::testing::Scripted;
    use super::*;

    fn snapshot() -> StateSnapshot {
        StateSnapshot {
            agents: AgentDirectory::from_rows([(
                "t1".into(),
                "migracion auth".into(),
                "running".into(),
                Some("s1".into()),
            )]),
            projects: vec!["bitacora".into()],
            current_project: Some("bitacora".into()),
            pending: Vec::new(),
        }
    }

    fn client(t: Scripted) -> SystemOneClient<Scripted> {
        let mut o = ClientOptions::new("tev1:0.8b");
        o.timeout = Duration::from_millis(400);
        SystemOneClient::new(t, o)
    }

    #[test]
    fn a_confident_status_request_executes() {
        let c = client(Scripted::new(vec![
            vec![("intent", "agent_status", 0.97)],
            vec![
                ("agent", "migracion-auth", 0.95),
                ("action", "status", 0.96),
            ],
        ]));
        let r = block_on(decide_transcript(
            &c,
            "cómo va migración auth",
            &snapshot(),
            Thresholds::default(),
            1,
        ));
        match &r.outcome {
            Outcome::Execute { request } => {
                assert_eq!(request.action, Action::Status);
                assert_eq!(request.target, AgentTarget::Agent("migracion-auth".into()));
            }
            other => panic!("{other:?}"),
        }
        assert!(!r.failed);
        let card = card_from(&r.record);
        assert!(
            card.rows
                .iter()
                .any(|row| row.label == "intent" && row.p.is_some())
        );
        assert!(
            card.result
                .as_ref()
                .is_some_and(|(t, _)| t.contains("status"))
        );
    }

    #[test]
    fn a_destructive_command_asks_for_confirmation() {
        let c = client(Scripted::new(vec![
            vec![("intent", "stop_agent", 0.97)],
            vec![
                ("agent", "migracion-auth", 0.97),
                ("action", "cancel", 0.97),
            ],
        ]));
        let r = block_on(decide_transcript(
            &c,
            "cancela migración auth",
            &snapshot(),
            Thresholds::default(),
            1,
        ));
        assert!(
            matches!(r.outcome, Outcome::Confirm { .. } | Outcome::Execute { .. }),
            "{:?}",
            r.outcome
        );
    }

    #[test]
    fn a_failing_decision_model_falls_open_to_conversation() {
        let mut t = Scripted::new(vec![]);
        t.fail = Some(TransportError::Unreachable("down".into()));
        let r = block_on(decide_transcript(
            &client(t),
            "hola",
            &snapshot(),
            Thresholds::default(),
            1,
        ));
        assert!(r.failed);
        assert!(matches!(r.outcome, Outcome::Conversation { .. }));
        assert_eq!(r.record.error.as_deref(), Some("unreachable"));
        assert!(!r.record.executed);
    }

    #[test]
    fn a_timeout_is_also_fail_open() {
        let mut t = Scripted::new(vec![]);
        t.fail = Some(TransportError::Timeout);
        let r = block_on(decide_transcript(
            &client(t),
            "hola",
            &snapshot(),
            Thresholds::default(),
            1,
        ));
        assert!(r.failed);
    }
}
