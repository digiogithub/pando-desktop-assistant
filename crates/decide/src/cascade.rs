//! The question cascade (`PDA-SP-0008.R1`): ask only what is needed.
//!
//! 0. A dictation lead-in ("dicta: …") decides `dictation` by rule, with no call.
//! 1. `intent` alone (fine-grained classes summed per intent).
//! 2. Only for `control`: `agent` + `action` in one request.
//! 3. Only for `spawn`: `engine` + `project` in one request.
//!
//! The `switch_project` intent class ("cambia a bitácora") asks `project` alone instead
//! of step 2.
//!
//! [`Cascade`] is a sans-IO state machine (`next_request` / `feed`) so it is testable
//! without a transport; [`run`] drives it over a [`SystemOneClient`].

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::DecisionError;
use crate::protocol::{Answer, Question, Request, Response};
use crate::questions::{
    Action, AgentTarget, Engine, Intent, ProjectChoice, action_question, agent_question,
    dictation_trigger, engine_question, intent_of_class, intent_question, names, project_question,
};
use crate::state::{StateInput, build_state, state_byte_budget};
use crate::transport::{DecisionTransport, SystemOneClient};

/// Intent class that routes to the project question (`PDA-SP-0008.R6`).
pub const SWITCH_PROJECT_CLASS: &str = "switch_project";

/// One answered question: the parsed value, `p(top)` and the ranked options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answered<T> {
    /// Parsed choice.
    pub value: T,
    /// Probability of the choice; `None` when missing or invalid (policy: 0).
    pub p: Option<f64>,
    /// Model-reported confidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Options by probability, descending (keys as on the wire).
    pub ranked: Vec<(String, f64)>,
}

impl<T> Answered<T> {
    fn from_answer(value: T, a: &Answer) -> Self {
        Self {
            value,
            p: a.p_top(),
            confidence: a.confidence,
            ranked: a.ranked(),
        }
    }

    /// `p`, with missing/invalid values as 0 (fail-closed).
    pub fn p_or_zero(&self) -> f64 {
        self.p.unwrap_or(0.0)
    }
}

/// Result of the cascade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// Step 1.
    pub intent: Answered<Intent>,
    /// Step 2 (control only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<Answered<AgentTarget>>,
    /// Step 3 (control only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Answered<Action>>,
    /// Spawn only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<Answered<Engine>>,
    /// Spawn only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<Answered<ProjectChoice>>,
    /// Fine-grained intent class chosen by the model (`stop_agent`, `question`, ...);
    /// `None` when the intent was decided by rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_class: Option<String>,
    /// For dictation: the text to insert (lead-in removed when decided by rule,
    /// otherwise the whole transcript).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dictation: Option<String>,
}

/// One request/response pair, for the decision log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exchange {
    /// What was asked.
    pub request: Request,
    /// What came back.
    pub response: Response,
    /// Round-trip latency in milliseconds.
    pub latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
enum Step {
    Intent,
    Target,
    Spawn,
    Project,
    Done,
}

/// Sans-IO cascade.
#[derive(Debug, Clone)]
pub struct Cascade {
    input: StateInput,
    model: String,
    keep_alive: Option<String>,
    context_tokens: usize,
    step: Step,
    intent: Option<Answered<Intent>>,
    intent_class: Option<String>,
    dictation: Option<String>,
    agent: Option<Answered<AgentTarget>>,
    action: Option<Answered<Action>>,
    engine: Option<Answered<Engine>>,
    project: Option<Answered<ProjectChoice>>,
}

impl Cascade {
    /// Start a cascade. More than 12 projects are fuzzy pre-filtered, and more than 24
    /// agents are reordered so the ones closest to the transcript are offered.
    pub fn new(
        input: StateInput,
        model: impl Into<String>,
        keep_alive: Option<String>,
        context_tokens: usize,
    ) -> Self {
        let mut input = input;
        // Long lists: keep the candidates closest to what was said (PDA-SP-0008.R6).
        if input.projects.len() > crate::project::PREFILTER_LIMIT {
            input.projects = crate::project::prefilter(
                &input.transcript,
                &input.projects,
                crate::project::PREFILTER_LIMIT,
            )
            .into_iter()
            .cloned()
            .collect();
        }
        if input.agents.len() > crate::questions::MAX_AGENT_CRITERIA {
            let scorer = crate::project::Scorer::new(&input.transcript);
            let key =
                |a: &crate::questions::AgentInfo| scorer.score(&a.id).max(scorer.score(&a.name));
            let mut ranked: Vec<(f64, usize)> = input
                .agents
                .iter()
                .enumerate()
                .map(|(i, a)| (key(a), i))
                .collect();
            ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
            let agents = std::mem::take(&mut input.agents);
            input.agents = ranked.into_iter().map(|(_, i)| agents[i].clone()).collect();
        }
        let dictation = dictation_trigger(&input.transcript);
        let intent = dictation.as_ref().map(|_| Answered {
            value: Intent::Dictation,
            p: Some(1.0),
            confidence: None,
            ranked: vec![(Intent::Dictation.key().to_string(), 1.0)],
        });
        Self {
            input,
            model: model.into(),
            keep_alive,
            context_tokens,
            step: if intent.is_some() {
                Step::Done
            } else {
                Step::Intent
            },
            intent,
            intent_class: None,
            dictation,
            agent: None,
            action: None,
            engine: None,
            project: None,
        }
    }

    fn questions(&self) -> Option<Vec<(String, Question)>> {
        Some(match self.step {
            Step::Intent => vec![(names::INTENT.into(), intent_question())],
            Step::Target => vec![
                (names::AGENT.into(), agent_question(&self.input.agents)),
                (names::ACTION.into(), action_question()),
            ],
            Step::Spawn => vec![
                (names::ENGINE.into(), engine_question()),
                (
                    names::PROJECT.into(),
                    project_question(&self.input.projects),
                ),
            ],
            Step::Project => vec![(
                names::PROJECT.into(),
                project_question(&self.input.projects),
            )],
            Step::Done => return None,
        })
    }

    /// The next request to send, or `None` when the cascade is complete.
    pub fn next_request(&self) -> Option<Request> {
        let questions = self.questions()?;
        let question_bytes = serde_json::to_vec(&Request {
            model: self.model.clone(),
            state: String::new(),
            questions: questions.clone(),
            keep_alive: self.keep_alive.clone(),
        })
        .map_or(0, |b| b.len());
        let budget = state_byte_budget(self.context_tokens, question_bytes);
        Some(Request {
            model: self.model.clone(),
            state: build_state(&self.input, budget),
            questions,
            keep_alive: self.keep_alive.clone(),
        })
    }

    /// Feed the (already checked) response to the current request.
    pub fn feed(&mut self, response: &Response) -> Result<(), DecisionError> {
        let get = |name: &str| {
            response.answers.get(name).ok_or_else(|| {
                DecisionError::Malformed(format!("answer for question {name:?} is missing"))
            })
        };
        let choice = |a: &Answer| a.choice.clone().unwrap_or_default();
        match self.step {
            Step::Intent => {
                let a = get(names::INTENT)?;
                let class = choice(a);
                let by_class = intent_of_class(&class).ok_or_else(|| {
                    DecisionError::Malformed(format!("unknown intent class {class:?}"))
                })?;
                let answered = aggregate_intent(a, by_class);
                let intent = answered.value;
                if intent == Intent::Dictation {
                    self.dictation = Some(self.input.transcript.trim().to_string());
                }
                let switch = class == SWITCH_PROJECT_CLASS;
                self.intent = Some(answered);
                self.intent_class = Some(class);
                self.step = match intent {
                    // "cambia a bitácora": a project question instead of agent/action.
                    Intent::Control if switch => Step::Project,
                    Intent::Control => Step::Target,
                    Intent::Conversation | Intent::Dictation => Step::Done,
                };
            }
            Step::Target => {
                let ag = get(names::AGENT)?;
                let ac = get(names::ACTION)?;
                let action = Action::from_key(&choice(ac)).ok_or_else(|| {
                    DecisionError::Malformed(format!("unknown action {:?}", ac.choice))
                })?;
                self.agent = Some(Answered::from_answer(
                    AgentTarget::from_key(&choice(ag)),
                    ag,
                ));
                self.action = Some(Answered::from_answer(action, ac));
                self.step = if action == Action::Spawn {
                    Step::Spawn
                } else {
                    Step::Done
                };
            }
            Step::Spawn => {
                let e = get(names::ENGINE)?;
                let p = get(names::PROJECT)?;
                let engine = Engine::from_key(&choice(e)).ok_or_else(|| {
                    DecisionError::Malformed(format!("unknown engine {:?}", e.choice))
                })?;
                self.engine = Some(Answered::from_answer(engine, e));
                self.project = Some(Answered::from_answer(
                    ProjectChoice::from_key(&choice(p)),
                    p,
                ));
                self.step = Step::Done;
            }
            Step::Project => {
                let p = get(names::PROJECT)?;
                self.project = Some(Answered::from_answer(
                    ProjectChoice::from_key(&choice(p)),
                    p,
                ));
                self.step = Step::Done;
            }
            Step::Done => {}
        }
        Ok(())
    }

    /// The intent, as soon as it is known (by rule or after the first response).
    pub fn intent(&self) -> Option<&Answered<Intent>> {
        self.intent.as_ref()
    }

    /// The decision, once complete.
    pub fn decision(&self) -> Option<Decision> {
        if self.step != Step::Done {
            return None;
        }
        Some(Decision {
            intent: self.intent.clone()?,
            agent: self.agent.clone(),
            action: self.action.clone(),
            engine: self.engine.clone(),
            project: self.project.clone(),
            intent_class: self.intent_class.clone(),
            dictation: self.dictation.clone(),
        })
    }
}

/// Sum class probabilities per intent and pick the most probable intent. When the
/// probabilities are unusable (missing, non-finite, out of range), fall back to the
/// chosen class's intent with `p = None`, which the policy reads as 0.
fn aggregate_intent(a: &Answer, by_class: Intent) -> Answered<Intent> {
    let mut sums: Vec<(Intent, f64)> = Intent::ALL.iter().map(|i| (*i, 0.0)).collect();
    let mut valid = !a.probabilities.is_empty();
    for (k, p) in &a.probabilities {
        if !p.is_finite() || !(0.0..=1.0).contains(p) {
            valid = false;
            continue;
        }
        if let Some(i) = intent_of_class(k)
            && let Some(slot) = sums.iter_mut().find(|(x, _)| *x == i)
        {
            slot.1 += p;
        }
    }
    sums.sort_by(|x, y| y.1.total_cmp(&x.1));
    let ranked: Vec<(String, f64)> = sums
        .iter()
        .map(|(i, p)| (i.key().to_string(), p.min(1.0)))
        .collect();
    match (valid, sums.first()) {
        (true, Some((top, p))) => Answered {
            value: *top,
            p: Some(p.min(1.0)),
            confidence: a.confidence,
            ranked,
        },
        _ => Answered {
            value: by_class,
            p: None,
            confidence: a.confidence,
            ranked,
        },
    }
}

/// A finished cascade: the decision and every exchange (for `decisions.jsonl`).
#[derive(Debug, Clone, PartialEq)]
pub struct CascadeRun {
    /// The decision.
    pub decision: Decision,
    /// Every request/response, in order.
    pub exchanges: Vec<Exchange>,
    /// Sum of round-trip latencies.
    pub latency: Duration,
}

/// A failed cascade: the error and the exchanges done before it. Fail-open: the caller
/// treats the transcript as conversation (`PDA-SP-0008.R4`).
#[derive(Debug, Clone, PartialEq)]
pub struct CascadeFailure {
    /// What went wrong.
    pub error: DecisionError,
    /// Exchanges completed before the failure.
    pub exchanges: Vec<Exchange>,
}

/// Drive the cascade over a client.
pub async fn run<T: DecisionTransport>(
    client: &SystemOneClient<T>,
    input: StateInput,
) -> Result<CascadeRun, CascadeFailure> {
    let opts = client.options();
    let mut cascade = Cascade::new(
        input,
        opts.model.clone(),
        opts.keep_alive.clone(),
        opts.context_tokens,
    );
    let mut exchanges = Vec::new();
    let mut latency = Duration::ZERO;
    while let Some(request) = cascade.next_request() {
        let fail = |error, exchanges| CascadeFailure { error, exchanges };
        let timed = match client.decide(&request).await {
            Ok(t) => t,
            Err(e) => return Err(fail(e, exchanges)),
        };
        latency += timed.latency;
        if let Err(e) = cascade.feed(&timed.value) {
            return Err(fail(e, exchanges));
        }
        exchanges.push(Exchange {
            request,
            response: timed.value,
            latency_ms: u64::try_from(timed.latency.as_millis()).unwrap_or(u64::MAX),
        });
    }
    match cascade.decision() {
        Some(decision) => Ok(CascadeRun {
            decision,
            exchanges,
            latency,
        }),
        None => Err(CascadeFailure {
            error: DecisionError::Malformed("cascade ended without a decision".into()),
            exchanges,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::questions::AgentInfo;
    use crate::transport::mock::{MockTransport, Reply};
    use crate::transport::{ClientOptions, TransportError, block_on};

    fn input(transcript: &str) -> StateInput {
        StateInput {
            transcript: transcript.into(),
            agents: vec![
                AgentInfo {
                    id: "migracion-auth".into(),
                    name: "Migración auth".into(),
                    status: "running".into(),
                },
                AgentInfo {
                    id: "bitacora-ui".into(),
                    name: "Bitácora UI".into(),
                    status: "waiting".into(),
                },
            ],
            projects: vec!["bitacora".into(), "pando".into()],
            current_project: Some("pando".into()),
        }
    }

    fn client(replies: Vec<Reply>) -> SystemOneClient<MockTransport> {
        SystemOneClient::new(MockTransport::new(replies), ClientOptions::new("tev1:0.8b"))
    }

    #[test]
    fn conversation_asks_only_intent() {
        let c = client(vec![Reply::Answer(vec![("intent", "question", 0.97)])]);
        let run = block_on(run(&c, input("explícame el error"))).unwrap();
        assert_eq!(run.decision.intent.value, Intent::Conversation);
        assert!(run.decision.action.is_none());
        assert_eq!(run.exchanges.len(), 1);
        let seen = c.transport().seen_json();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["questions"].as_object().unwrap().len(), 1);
    }

    /// Scenario PDA-SP-0008.R1 "control": "cancela migración auth".
    #[test]
    fn control_scenario_cancel_migracion_auth() {
        let c = client(vec![
            Reply::Answer(vec![("intent", "stop_agent", 0.93)]),
            Reply::Answer(vec![
                ("agent", "migracion-auth", 0.91),
                ("action", "cancel", 0.96),
            ]),
        ]);
        let run = block_on(run(&c, input("cancela migración auth"))).unwrap();
        let d = run.decision;
        assert_eq!(d.intent.value, Intent::Control);
        // Mock spreads the rest evenly, so other control classes add to 0.93.
        assert!(d.intent.p.unwrap() >= 0.93);
        assert_eq!(
            d.agent.as_ref().map(|a| &a.value),
            Some(&AgentTarget::Agent("migracion-auth".into()))
        );
        assert_eq!(d.agent.as_ref().and_then(|a| a.p), Some(0.91));
        assert_eq!(d.action.as_ref().map(|a| a.value), Some(Action::Cancel));
        assert_eq!(d.action.as_ref().and_then(|a| a.p), Some(0.96));
        assert!(d.engine.is_none() && d.project.is_none());

        let seen = c.transport().seen_json();
        assert_eq!(seen.len(), 2);
        let q = seen[1]["questions"].as_object().unwrap();
        let agent_keys: Vec<_> = q["agent"]["criteria"].as_object().unwrap().keys().collect();
        assert_eq!(agent_keys, ["migracion-auth", "bitacora-ui", "all", "none"]);
        assert_eq!(q["agent"]["criteria"]["migracion-auth"], "Migración auth");
        // The state is a JSON-object string carrying the transcript and live lists.
        let state: serde_json::Value =
            serde_json::from_str(seen[1]["state"].as_str().unwrap()).unwrap();
        assert_eq!(state["transcript"], "cancela migración auth");
        assert_eq!(state["agents"][0]["id"], "migracion-auth");
    }

    #[test]
    fn dictation_lead_in_needs_no_call() {
        let c = client(vec![]);
        let run = block_on(run(&c, input("dicta: añade un test para el parser"))).unwrap();
        assert_eq!(run.decision.intent.value, Intent::Dictation);
        assert_eq!(run.decision.intent.p, Some(1.0));
        assert_eq!(
            run.decision.dictation.as_deref(),
            Some("añade un test para el parser")
        );
        assert!(run.exchanges.is_empty());
        assert!(c.transport().seen_json().is_empty());
    }

    #[test]
    fn intent_probabilities_are_summed_per_intent() {
        // The top class is `question` (0.4) but control classes sum to 0.55.
        let body = r#"{"answers":{"intent":{"choice":"question","probabilities":{
            "question":0.4,"stop_agent":0.3,"agent_status":0.25,"dictate":0.05}}}}"#;
        let c = client(vec![
            Reply::Ok(body.into()),
            Reply::Answer(vec![("agent", "none", 0.9), ("action", "status", 0.9)]),
        ]);
        let run = block_on(run(&c, input("¿cómo va?"))).unwrap();
        assert_eq!(run.decision.intent.value, Intent::Control);
        assert!((run.decision.intent.p.unwrap() - 0.55).abs() < 1e-9);
        assert_eq!(run.decision.intent_class.as_deref(), Some("question"));
        assert_eq!(run.decision.intent.ranked[0].0, "control");
    }

    #[test]
    fn invalid_intent_probabilities_fail_closed() {
        let body =
            r#"{"answers":{"intent":{"choice":"stop_agent","probabilities":{"stop_agent":7.0}}}}"#;
        let c = client(vec![
            Reply::Ok(body.into()),
            Reply::Answer(vec![("agent", "none", 0.9), ("action", "cancel", 0.9)]),
        ]);
        let run = block_on(run(&c, input("cancela"))).unwrap();
        assert_eq!(run.decision.intent.value, Intent::Control);
        assert_eq!(run.decision.intent.p, None);
    }

    #[test]
    fn long_lists_keep_the_mentioned_candidates() {
        let mut inp = input("cancela el agente cuarenta y dos de facturas");
        inp.agents = (0..40)
            .map(|i| AgentInfo {
                id: format!("agente-{i}"),
                name: if i == 37 {
                    "Facturas".into()
                } else {
                    format!("Agente {i}")
                },
                status: String::new(),
            })
            .collect();
        inp.projects = (0..30)
            .map(|i| format!("proyecto-{i}"))
            .chain(["facturas".into()])
            .collect();
        let mut cascade = Cascade::new(inp, "m", None, 2050);
        let intent =
            r#"{"answers":{"intent":{"choice":"stop_agent","probabilities":{"stop_agent":0.9}}}}"#;
        cascade
            .feed(&serde_json::from_str(intent).unwrap())
            .unwrap();
        let req = cascade.next_request().unwrap();
        let agents = req.question("agent").unwrap();
        assert!(agents.keys().any(|k| k == "agente-37"));
        assert!(cascade.input.projects.len() <= 12);
        assert_eq!(cascade.input.projects[0], "facturas");
    }

    #[test]
    fn switch_project_asks_only_the_project() {
        let c = client(vec![
            Reply::Answer(vec![("intent", "switch_project", 0.9)]),
            Reply::Answer(vec![("project", "bitacora", 0.93)]),
        ]);
        let run = block_on(run(&c, input("cambia a bitácora"))).unwrap();
        let d = run.decision;
        assert_eq!(d.intent.value, Intent::Control);
        assert!(d.action.is_none() && d.agent.is_none());
        assert_eq!(
            d.project.as_ref().map(|p| &p.value),
            Some(&ProjectChoice::Named("bitacora".into()))
        );
        let seen = c.transport().seen_json();
        assert_eq!(seen[1]["questions"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn spawn_asks_engine_and_project() {
        let c = client(vec![
            Reply::Answer(vec![("intent", "stop_agent", 0.9)]),
            Reply::Answer(vec![("agent", "none", 0.8), ("action", "spawn", 0.9)]),
            Reply::Answer(vec![
                ("engine", "claude-code", 0.88),
                ("project", "bitacora", 0.93),
            ]),
        ]);
        let run = block_on(run(&c, input("lanza un claude en bitácora"))).unwrap();
        let d = run.decision;
        assert_eq!(d.engine.map(|e| e.value), Some(Engine::ClaudeCode));
        assert_eq!(
            d.project.map(|p| p.value),
            Some(ProjectChoice::Named("bitacora".into()))
        );
        assert_eq!(run.exchanges.len(), 3);
        let seen = c.transport().seen_json();
        let keys: Vec<_> = seen[2]["questions"]["project"]["criteria"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        assert_eq!(keys, ["bitacora", "pando", "current"]);
    }

    #[test]
    fn failures_are_reported_with_partial_exchanges() {
        let c = client(vec![
            Reply::Answer(vec![("intent", "stop_agent", 0.9)]),
            Reply::Fail(TransportError::Timeout),
        ]);
        let f = block_on(run(&c, input("cancela"))).unwrap_err();
        assert_eq!(f.error, DecisionError::Timeout);
        assert_eq!(f.exchanges.len(), 1);

        let c = client(vec![Reply::Fail(TransportError::Unreachable(
            "down".into(),
        ))]);
        let f = block_on(run(&c, input("hola"))).unwrap_err();
        assert_eq!(f.error.kind(), "unreachable");
        assert!(f.exchanges.is_empty());
    }

    #[test]
    fn every_request_respects_context_and_body_budget() {
        let mut inp = input(&"muy largo ".repeat(3000));
        inp.agents = (0..40)
            .map(|i| AgentInfo {
                id: format!("agente-{i}"),
                name: format!("Agente {i}"),
                status: "running".into(),
            })
            .collect();
        inp.projects = (0..200).map(|i| format!("proyecto-{i}")).collect();
        let mut cascade = Cascade::new(inp, "tev1:0.8b", None, 2050);
        for picks in [
            vec![("intent", "stop_agent", 0.9)],
            vec![("agent", "agente-1", 0.9), ("action", "spawn", 0.9)],
            vec![("engine", "pando", 0.9), ("project", "current", 0.9)],
        ] {
            let req = cascade.next_request().unwrap();
            let body = req.encode().unwrap();
            assert!(
                crate::state::estimate_tokens(body.len()) <= 2050,
                "{}",
                body.len()
            );
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let resp: Response =
                serde_json::from_str(&crate::transport::mock::answer_body(&json, &picks)).unwrap();
            cascade.feed(&resp).unwrap();
        }
        assert!(cascade.next_request().is_none());
        assert!(cascade.decision().is_some());
    }
}
