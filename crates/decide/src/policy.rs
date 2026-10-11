//! Trust policy (`PDA-SP-0008.R2`): the code, not the model, decides what runs.
//!
//! | p(top)      | Outcome |
//! |-------------|---------|
//! | ≥ 0.85      | execute |
//! | 0.6 – 0.85  | execute if reversible (pause/status/open), otherwise confirm |
//! | < 0.6       | conversation, top-2 options as buttons |
//!
//! Destructive actions (`cancel`, `deny`) always confirm unless p ≥ 0.95 **and** the
//! agent id literally appears in the transcript.
//!
//! Fail-closed rules on top of the table:
//! - `p` of a control decision is the **minimum** over every question it depends on
//!   (intent, agent when the action needs one, action, engine/project for spawn); a
//!   missing or invalid probability counts as 0.
//! - `approve` never executes on confidence when a pending item of the target is flagged
//!   `require_explicit_approval` / `never_auto_approve`: the outcome is
//!   [`Outcome::ExplicitOnly`] and only an explicit UI decision may approve it.
//! - Commands on `all` agents, approvals of an implied target and every other
//!   irreversible action below the execute threshold confirm.
//! - An unknown agent id or a missing target never executes.
//! - Thresholds are configurable but clamped to floors ([`Thresholds::sanitized`]).

use serde::{Deserialize, Serialize};

use crate::cascade::{Answered, Decision};
use crate::questions::{Action, AgentInfo, AgentTarget, Engine, Intent, ProjectChoice};
use crate::text::contains_words;

/// Policy thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    /// p at or above which any non-destructive command executes (default 0.85).
    pub execute: f64,
    /// p below which everything is conversation; reversible commands execute from
    /// here (default 0.6).
    pub reversible: f64,
    /// p at or above which a destructive command may execute when its id is literal
    /// (default 0.95).
    pub destructive: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            execute: 0.85,
            reversible: 0.6,
            destructive: 0.95,
        }
    }
}

/// Lowest values a configuration may set; lower values are raised to these.
pub const FLOORS: Thresholds = Thresholds {
    execute: 0.75,
    reversible: 0.5,
    destructive: 0.9,
};

impl Thresholds {
    /// Clamp to `[floor, 1]`, replace non-finite values by the default, and keep
    /// `reversible ≤ execute ≤ destructive`.
    pub fn sanitized(self) -> Self {
        let d = Self::default();
        let fix = |v: f64, floor: f64, def: f64| {
            if v.is_finite() {
                v.clamp(floor, 1.0)
            } else {
                def
            }
        };
        let reversible = fix(self.reversible, FLOORS.reversible, d.reversible);
        let execute = fix(self.execute, FLOORS.execute, d.execute).max(reversible);
        let destructive = fix(self.destructive, FLOORS.destructive, d.destructive).max(execute);
        Self {
            execute,
            reversible,
            destructive,
        }
    }
}

/// A pending permission/question of an agent, with its safety flags.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PendingApproval {
    /// Agent (session/task) the item belongs to.
    pub agent_id: String,
    /// `require_explicit_approval` or `never_auto_approve` is set on the item.
    pub explicit_only: bool,
}

/// Everything the policy looks at besides the decision.
#[derive(Debug, Clone, Copy)]
pub struct PolicyContext<'a> {
    /// The transcript (for the literal-id rule).
    pub transcript: &'a str,
    /// Live agents; a target outside this list never executes.
    pub agents: &'a [AgentInfo],
    /// Pending approvals.
    pub pending: &'a [PendingApproval],
    /// Thresholds (sanitized before use).
    pub thresholds: Thresholds,
}

/// A resolved control command, ready for the executor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlRequest {
    /// What to do.
    pub action: Action,
    /// On which agent(s). Spawn uses `None`.
    pub target: AgentTarget,
    /// Spawn engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<Engine>,
    /// Spawn project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectChoice>,
    /// The combined probability the policy used.
    pub p: f64,
}

/// Why a command needs a confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmReason {
    /// Irreversible and below the execute threshold.
    NotReversible,
    /// Destructive without p ≥ destructive threshold and the literal id.
    Destructive,
    /// Targets every agent.
    AllAgents,
    /// The target was inferred (single pending approval), not said.
    ImpliedTarget,
}

/// Why the transcript goes to the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationReason {
    /// The intent is conversation.
    Intent,
    /// Some needed answer is below the reversible threshold: show `top2` as buttons.
    LowConfidence,
    /// The command needs an agent and none (or an unknown one) was identified.
    MissingTarget,
    /// The decision layer failed (fail-open): show the "decisión: no disponible" chip.
    Unavailable,
}

/// The policy's verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    /// Run the command now.
    Execute {
        /// The command.
        request: ControlRequest,
    },
    /// Ask "¿Cancelo migracion-auth?" first; run only on an explicit yes.
    Confirm {
        /// The command.
        request: ControlRequest,
        /// Why.
        reason: ConfirmReason,
    },
    /// An approval of a flagged item: never approve by voice confidence; show the
    /// approval UI and wait for an explicit user decision.
    ExplicitOnly {
        /// The command (always `approve`).
        request: ControlRequest,
    },
    /// Insert the text into the composer of the voice target without sending.
    Dictate {
        /// Text to insert.
        text: String,
    },
    /// Send the transcript to the chat.
    Conversation {
        /// Why.
        reason: ConversationReason,
        /// Top-2 options of the weakest question (for `LowConfidence`).
        top2: Vec<(String, f64)>,
    },
}

impl Outcome {
    /// Fail-open outcome when the decision layer is unavailable (`PDA-SP-0008.R4`).
    pub fn unavailable() -> Self {
        Self::Conversation {
            reason: ConversationReason::Unavailable,
            top2: Vec::new(),
        }
    }

    /// Short stable label for logs.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Execute { .. } => "execute",
            Self::Confirm { .. } => "confirm",
            Self::ExplicitOnly { .. } => "explicit_only",
            Self::Dictate { .. } => "dictate",
            Self::Conversation { .. } => "conversation",
        }
    }
}

fn p_of<T>(a: Option<&Answered<T>>) -> f64 {
    a.and_then(|a| a.p)
        .filter(|p| p.is_finite() && (0.0..=1.0).contains(p))
        .unwrap_or(0.0)
}

fn top2<T>(a: Option<&Answered<T>>) -> Vec<(String, f64)> {
    a.map(|a| a.ranked.iter().take(2).cloned().collect())
        .unwrap_or_default()
}

fn low(top2: Vec<(String, f64)>) -> Outcome {
    Outcome::Conversation {
        reason: ConversationReason::LowConfidence,
        top2,
    }
}

/// Decide what to do with a decision. Pure and total: every input maps to an outcome,
/// and anything unexpected falls to the safe side.
pub fn policy(decision: &Decision, ctx: &PolicyContext<'_>) -> Outcome {
    let t = ctx.thresholds.sanitized();
    let p_intent = p_of(Some(&decision.intent));

    match decision.intent.value {
        Intent::Conversation => {
            return Outcome::Conversation {
                reason: ConversationReason::Intent,
                top2: Vec::new(),
            };
        }
        Intent::Dictation => {
            return if p_intent >= t.reversible {
                Outcome::Dictate {
                    text: decision
                        .dictation
                        .clone()
                        .unwrap_or_else(|| ctx.transcript.trim().to_string()),
                }
            } else {
                low(top2(Some(&decision.intent)))
            };
        }
        Intent::Control => {}
    }

    // Every question the command depends on, with its probability; the weakest one
    // drives the outcome and provides the buttons.
    let Some(action_a) = decision.action.as_ref() else {
        return low(top2(Some(&decision.intent)));
    };
    let action = action_a.value;
    let mut parts: Vec<(f64, Vec<(String, f64)>)> = vec![
        (p_intent, top2(Some(&decision.intent))),
        (p_of(Some(action_a)), top2(Some(action_a))),
    ];
    if action.needs_agent() {
        parts.push((p_of(decision.agent.as_ref()), top2(decision.agent.as_ref())));
    } else {
        parts.push((
            p_of(decision.engine.as_ref()),
            top2(decision.engine.as_ref()),
        ));
        parts.push((
            p_of(decision.project.as_ref()),
            top2(decision.project.as_ref()),
        ));
    }
    let (p, weakest) = parts
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((0.0, Vec::new()));
    if p < t.reversible {
        return low(weakest);
    }

    // Resolve and validate the target.
    let said = decision
        .agent
        .as_ref()
        .map_or(AgentTarget::None, |a| a.value.clone());
    let mut implied = false;
    let target = if !action.needs_agent() {
        AgentTarget::None
    } else {
        match said {
            AgentTarget::Agent(id) if ctx.agents.iter().any(|a| a.id == id) => {
                AgentTarget::Agent(id)
            }
            AgentTarget::Agent(_) => {
                return Outcome::Conversation {
                    reason: ConversationReason::MissingTarget,
                    top2: top2(decision.agent.as_ref()),
                };
            }
            AgentTarget::All => AgentTarget::All,
            AgentTarget::None => match action {
                // Overall status / open the board need no specific agent.
                Action::Status | Action::Open => AgentTarget::None,
                Action::Approve | Action::Deny => {
                    let mut owners: Vec<&str> =
                        ctx.pending.iter().map(|p| p.agent_id.as_str()).collect();
                    owners.sort_unstable();
                    owners.dedup();
                    match owners.as_slice() {
                        [only] => {
                            implied = true;
                            AgentTarget::Agent((*only).to_string())
                        }
                        _ => {
                            return Outcome::Conversation {
                                reason: ConversationReason::MissingTarget,
                                top2: top2(decision.agent.as_ref()),
                            };
                        }
                    }
                }
                _ => {
                    return Outcome::Conversation {
                        reason: ConversationReason::MissingTarget,
                        top2: top2(decision.agent.as_ref()),
                    };
                }
            },
        }
    };

    let request = ControlRequest {
        action,
        target: target.clone(),
        engine: decision.engine.as_ref().map(|e| e.value),
        project: decision.project.as_ref().map(|p| p.value.clone()),
        p,
    };

    // Flagged approvals never pass on confidence.
    if action == Action::Approve {
        let flagged = ctx.pending.iter().any(|item| {
            item.explicit_only
                && match &target {
                    AgentTarget::Agent(id) => item.agent_id == *id,
                    AgentTarget::All | AgentTarget::None => true,
                }
        });
        if flagged {
            return Outcome::ExplicitOnly { request };
        }
    }

    let confirm = |reason| Outcome::Confirm {
        request: request.clone(),
        reason,
    };
    if target == AgentTarget::All && !matches!(action, Action::Status | Action::Open) {
        return confirm(ConfirmReason::AllAgents);
    }
    if implied {
        return confirm(ConfirmReason::ImpliedTarget);
    }
    if action.is_destructive() {
        let literal = match &target {
            AgentTarget::Agent(id) => contains_words(ctx.transcript, id),
            AgentTarget::All | AgentTarget::None => false,
        };
        return if p >= t.destructive && literal {
            Outcome::Execute { request }
        } else {
            confirm(ConfirmReason::Destructive)
        };
    }
    if p >= t.execute || action.is_reversible() {
        Outcome::Execute { request }
    } else {
        confirm(ConfirmReason::NotReversible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn ans<T>(value: T, p: f64) -> Answered<T> {
        Answered {
            value,
            p: Some(p),
            confidence: None,
            ranked: vec![("top".into(), p), ("second".into(), 1.0 - p)],
        }
    }

    fn agents() -> Vec<AgentInfo> {
        ["migracion-auth", "bitacora-ui"]
            .iter()
            .map(|id| AgentInfo {
                id: (*id).into(),
                ..AgentInfo::default()
            })
            .collect()
    }

    fn control(action: Action, target: AgentTarget, p: f64) -> Decision {
        Decision {
            intent: ans(Intent::Control, 0.99),
            agent: Some(ans(target, 0.99)),
            action: Some(ans(action, p)),
            engine: None,
            project: None,
            intent_class: None,
            dictation: None,
        }
    }

    fn run(d: &Decision, transcript: &str, pending: &[PendingApproval]) -> Outcome {
        let agents = agents();
        policy(
            d,
            &PolicyContext {
                transcript,
                agents: &agents,
                pending,
                thresholds: Thresholds::default(),
            },
        )
    }

    fn mig() -> AgentTarget {
        AgentTarget::Agent("migracion-auth".into())
    }

    /// The exhaustive table of PDA-SP-0008.R2:
    /// (action, p, literal id in transcript, expected outcome label, confirm reason).
    #[test]
    fn spec_table() {
        use Action::*;
        let lit = "cancela migración auth";
        let not_lit = "cancela la migración";
        #[rustfmt::skip]
        let cases: &[(Action, f64, &str, &str, Option<ConfirmReason>)] = &[
            // < 0.6: conversation for every action.
            (Pause, 0.59, lit, "conversation", None),
            (Status, 0.3, lit, "conversation", None),
            (Cancel, 0.59, lit, "conversation", None),
            (Spawn, 0.1, lit, "conversation", None),
            // 0.6 – 0.85: reversible executes.
            (Pause, 0.6, lit, "execute", None),
            (Status, 0.7, lit, "execute", None),
            (Open, 0.84, lit, "execute", None),
            // 0.6 – 0.85: irreversible confirms.
            (Resume, 0.7, lit, "confirm", Some(ConfirmReason::NotReversible)),
            (Retry, 0.84, lit, "confirm", Some(ConfirmReason::NotReversible)),
            (Approve, 0.7, lit, "confirm", Some(ConfirmReason::NotReversible)),
            // ≥ 0.85: executes.
            (Resume, 0.85, lit, "execute", None),
            (Retry, 0.9, lit, "execute", None),
            (Approve, 0.9, lit, "execute", None),
            (Status, 0.99, lit, "execute", None),
            // Destructive: confirm unless p ≥ 0.95 and literal id.
            (Cancel, 0.7, lit, "confirm", Some(ConfirmReason::Destructive)),
            (Cancel, 0.9, lit, "confirm", Some(ConfirmReason::Destructive)),
            (Cancel, 0.9, not_lit, "confirm", Some(ConfirmReason::Destructive)),
            (Cancel, 0.97, not_lit, "confirm", Some(ConfirmReason::Destructive)),
            (Cancel, 0.95, lit, "execute", None),
            (Cancel, 0.99, lit, "execute", None),
            (Deny, 0.9, lit, "confirm", Some(ConfirmReason::Destructive)),
            (Deny, 0.97, not_lit, "confirm", Some(ConfirmReason::Destructive)),
            (Deny, 0.97, "deniega migracion-auth", "execute", None),
        ];
        for (action, p, transcript, label, reason) in cases {
            let out = run(&control(*action, mig(), *p), transcript, &[]);
            assert_eq!(
                out.label(),
                *label,
                "{action:?} p={p} {transcript:?}: {out:?}"
            );
            if let Outcome::Confirm { reason: r, .. } = &out {
                assert_eq!(Some(*r), *reason, "{action:?} p={p}");
            }
        }
    }

    /// Scenario "ambiguous cancel": p 0.9, id not literal → confirm before the API.
    #[test]
    fn ambiguous_cancel_confirms() {
        let out = run(
            &control(Action::Cancel, mig(), 0.9),
            "cancela la de auth",
            &[],
        );
        assert!(matches!(
            out,
            Outcome::Confirm {
                reason: ConfirmReason::Destructive,
                ..
            }
        ));
    }

    #[test]
    fn low_confidence_shows_top2_of_weakest_question() {
        let mut d = control(Action::Status, mig(), 0.95);
        d.agent = Some(Answered {
            value: mig(),
            p: Some(0.4),
            confidence: None,
            ranked: vec![
                ("migracion-auth".into(), 0.4),
                ("bitacora-ui".into(), 0.35),
                ("none".into(), 0.25),
            ],
        });
        let out = run(&d, "¿cómo va?", &[]);
        assert_eq!(
            out,
            Outcome::Conversation {
                reason: ConversationReason::LowConfidence,
                top2: vec![("migracion-auth".into(), 0.4), ("bitacora-ui".into(), 0.35)],
            }
        );
    }

    #[test]
    fn intent_and_dictation() {
        let mut d = control(Action::Status, mig(), 0.99);
        d.intent = ans(Intent::Conversation, 0.99);
        assert_eq!(run(&d, "x", &[]).label(), "conversation");
        d.intent = ans(Intent::Dictation, 0.7);
        d.dictation = Some("añade un test".into());
        assert_eq!(
            run(&d, "dicta añade un test", &[]),
            Outcome::Dictate {
                text: "añade un test".into()
            }
        );
        d.intent = ans(Intent::Dictation, 0.5);
        assert_eq!(run(&d, "x", &[]).label(), "conversation");
        d.intent = ans(Intent::Control, 0.5);
        assert_eq!(run(&d, "x", &[]).label(), "conversation");
    }

    #[test]
    fn flagged_approvals_are_explicit_only() {
        let pending = [PendingApproval {
            agent_id: "migracion-auth".into(),
            explicit_only: true,
        }];
        let out = run(
            &control(Action::Approve, mig(), 1.0),
            "aprueba migracion-auth",
            &pending,
        );
        assert_eq!(out.label(), "explicit_only");
        // Approving `all` while any item is flagged is explicit-only too.
        let out = run(
            &control(Action::Approve, AgentTarget::All, 1.0),
            "aprueba todo",
            &pending,
        );
        assert_eq!(out.label(), "explicit_only");
        // Implied target with a flagged item.
        let out = run(
            &control(Action::Approve, AgentTarget::None, 1.0),
            "aprueba",
            &pending,
        );
        assert_eq!(out.label(), "explicit_only");
        // A flagged item of another agent does not block this one.
        let out = run(
            &control(
                Action::Approve,
                AgentTarget::Agent("bitacora-ui".into()),
                0.9,
            ),
            "aprueba",
            &pending,
        );
        assert_eq!(out.label(), "execute");
        // Deny of a flagged item follows the destructive rule.
        let out = run(&control(Action::Deny, mig(), 0.9), "deniega", &pending);
        assert_eq!(out.label(), "confirm");
    }

    #[test]
    fn targets() {
        // Unknown id: never execute.
        let d = control(Action::Status, AgentTarget::Agent("ghost".into()), 0.99);
        assert_eq!(
            run(&d, "x", &[]),
            Outcome::Conversation {
                reason: ConversationReason::MissingTarget,
                top2: vec![("top".into(), 0.99), ("second".into(), 1.0 - 0.99)],
            }
        );
        // No target for a command that needs one.
        let d = control(Action::Cancel, AgentTarget::None, 0.99);
        assert_eq!(run(&d, "cancela", &[]).label(), "conversation");
        // Overall status without a target is fine.
        let d = control(Action::Status, AgentTarget::None, 0.9);
        assert_eq!(run(&d, "¿cómo van?", &[]).label(), "execute");
        // `all` confirms everything except status/open.
        for a in [
            Action::Cancel,
            Action::Approve,
            Action::Retry,
            Action::Pause,
        ] {
            let out = run(&control(a, AgentTarget::All, 1.0), "todos", &[]);
            assert!(
                matches!(
                    out,
                    Outcome::Confirm {
                        reason: ConfirmReason::AllAgents,
                        ..
                    }
                ),
                "{a:?}: {out:?}"
            );
        }
        let out = run(
            &control(Action::Status, AgentTarget::All, 0.9),
            "todos",
            &[],
        );
        assert_eq!(out.label(), "execute");
        // Implied approval target (one pending owner) confirms.
        let pending = [
            PendingApproval {
                agent_id: "bitacora-ui".into(),
                explicit_only: false,
            },
            PendingApproval {
                agent_id: "bitacora-ui".into(),
                explicit_only: false,
            },
        ];
        let out = run(
            &control(Action::Approve, AgentTarget::None, 1.0),
            "aprueba",
            &pending,
        );
        match out {
            Outcome::Confirm {
                request,
                reason: ConfirmReason::ImpliedTarget,
            } => {
                assert_eq!(request.target, AgentTarget::Agent("bitacora-ui".into()));
            }
            other => panic!("{other:?}"),
        }
        // Two owners: ambiguous.
        let pending = [
            PendingApproval {
                agent_id: "bitacora-ui".into(),
                explicit_only: false,
            },
            PendingApproval {
                agent_id: "migracion-auth".into(),
                explicit_only: false,
            },
        ];
        let out = run(
            &control(Action::Approve, AgentTarget::None, 1.0),
            "aprueba",
            &pending,
        );
        assert_eq!(out.label(), "conversation");
    }

    #[test]
    fn spawn_uses_engine_and_project_confidence() {
        let mut d = control(Action::Spawn, AgentTarget::None, 0.95);
        d.agent = Some(ans(AgentTarget::None, 0.1)); // irrelevant for spawn
        d.engine = Some(ans(Engine::Pando, 0.95));
        d.project = Some(ans(ProjectChoice::Named("bitacora".into()), 0.7));
        let out = run(&d, "lanza un agente en bitácora", &[]);
        assert!(matches!(
            out,
            Outcome::Confirm {
                reason: ConfirmReason::NotReversible,
                ..
            }
        ));
        d.project = Some(ans(ProjectChoice::Named("bitacora".into()), 0.9));
        let out = run(&d, "lanza un agente en bitácora", &[]);
        match out {
            Outcome::Execute { request } => {
                assert_eq!(request.engine, Some(Engine::Pando));
                assert_eq!(
                    request.project,
                    Some(ProjectChoice::Named("bitacora".into()))
                );
                assert_eq!(request.target, AgentTarget::None);
            }
            other => panic!("{other:?}"),
        }
        d.project = None;
        assert_eq!(run(&d, "lanza", &[]).label(), "conversation");
    }

    #[test]
    fn missing_or_invalid_probabilities_fail_closed() {
        let mut d = control(Action::Status, mig(), 0.99);
        d.intent.p = None;
        assert_eq!(run(&d, "x", &[]).label(), "conversation");
        let mut d = control(Action::Status, mig(), 0.99);
        d.action = None;
        assert_eq!(run(&d, "x", &[]).label(), "conversation");
        let mut d = control(Action::Status, mig(), f64::NAN);
        assert_eq!(run(&d, "x", &[]).label(), "conversation");
        d = control(Action::Status, mig(), 3.0);
        assert_eq!(run(&d, "x", &[]).label(), "conversation");
        d = control(Action::Status, mig(), 0.99);
        d.agent = None;
        assert_eq!(run(&d, "x", &[]).label(), "conversation");
    }

    #[test]
    fn thresholds_have_floors() {
        let t = Thresholds {
            execute: 0.1,
            reversible: -3.0,
            destructive: f64::NAN,
        }
        .sanitized();
        assert_eq!(t.execute, FLOORS.execute);
        assert_eq!(t.reversible, FLOORS.reversible);
        assert_eq!(t.destructive, 0.95);
        let t = Thresholds {
            execute: 0.99,
            reversible: 0.995,
            destructive: 0.9,
        }
        .sanitized();
        assert!(t.reversible <= t.execute && t.execute <= t.destructive);
        assert_eq!(Thresholds::default().sanitized(), Thresholds::default());
    }

    #[test]
    fn unavailable_is_conversation() {
        assert_eq!(
            Outcome::unavailable(),
            Outcome::Conversation {
                reason: ConversationReason::Unavailable,
                top2: vec![]
            }
        );
    }

    fn arb_target() -> impl Strategy<Value = AgentTarget> {
        prop_oneof![
            Just(AgentTarget::All),
            Just(AgentTarget::None),
            Just(mig()),
            Just(AgentTarget::Agent("bitacora-ui".into())),
            Just(AgentTarget::Agent("ghost".into())),
        ]
    }

    fn arb_p() -> impl Strategy<Value = Option<f64>> {
        prop_oneof![
            8 => (0.0f64..=1.0).prop_map(Some),
            1 => Just(None),
            1 => prop_oneof![Just(f64::NAN), Just(-0.5), Just(1.5), Just(f64::INFINITY)].prop_map(Some),
        ]
    }

    prop_compose! {
        fn arb_decision()(
            intent in prop::sample::select(Intent::ALL.to_vec()),
            pi in arb_p(),
            action in prop::option::of(prop::sample::select(Action::ALL.to_vec())),
            pa in arb_p(),
            target in arb_target(),
            pt in arb_p(),
            pe in arb_p(),
            pp in arb_p(),
        ) -> Decision {
            fn a<T>(value: T, p: Option<f64>) -> Answered<T> {
                Answered { value, p, confidence: None, ranked: vec![] }
            }
            Decision {
                intent: a(intent, pi),
                agent: Some(Answered { value: target, p: pt, confidence: None, ranked: vec![] }),
                action: action.map(|x| a(x, pa)),
                engine: Some(Answered { value: Engine::Pando, p: pe, confidence: None, ranked: vec![] }),
                project: Some(Answered { value: ProjectChoice::Current, p: pp, confidence: None, ranked: vec![] }),
                intent_class: None,
                dictation: None,
            }
        }
    }

    fn arb_pending() -> impl Strategy<Value = Vec<PendingApproval>> {
        prop::collection::vec(
            (
                prop::sample::select(vec!["migracion-auth", "bitacora-ui", "ghost"]),
                any::<bool>(),
            )
                .prop_map(|(id, f)| PendingApproval {
                    agent_id: id.into(),
                    explicit_only: f,
                }),
            0..4,
        )
    }

    fn valid_p<T>(a: Option<&Answered<T>>) -> f64 {
        p_of(a)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(4000))]

        /// Nothing executes when any needed probability is below the reversible
        /// threshold, missing or invalid.
        #[test]
        fn never_executes_below_threshold(d in arb_decision(), pending in arb_pending(), transcript in "(cancela|deniega|aprueba)( migracion auth| bitacora ui)?") {
            let out = run(&d, &transcript, &pending);
            if let Outcome::Execute { request } = &out {
                let t = Thresholds::default();
                prop_assert!(valid_p(Some(&d.intent)) >= t.reversible);
                prop_assert!(valid_p(d.action.as_ref()) >= t.reversible);
                prop_assert!(request.p >= t.reversible);
                if request.action.needs_agent() {
                    prop_assert!(valid_p(d.agent.as_ref()) >= t.reversible);
                }
                if !request.action.is_reversible() {
                    prop_assert!(request.p >= t.execute);
                }
            }
        }

        /// Flagged approvals never execute, whatever the confidence.
        #[test]
        fn flagged_approval_never_executes(d in arb_decision(), pending in arb_pending()) {
            let out = run(&d, "aprueba migracion auth bitacora ui", &pending);
            if let Outcome::Execute { request } = &out
                && request.action == Action::Approve
            {
                let blocked = pending.iter().any(|p| p.explicit_only && match &request.target {
                    AgentTarget::Agent(id) => &p.agent_id == id,
                    _ => true,
                });
                prop_assert!(!blocked, "{out:?} with {pending:?}");
            }
        }

        /// Destructive commands execute only with p ≥ 0.95 and the literal id.
        #[test]
        fn destructive_needs_literal_id(d in arb_decision(), pending in arb_pending(), transcript in "(cancela|deniega)( la de| migracion auth| bitacora ui| todo)?") {
            if let Outcome::Execute { request } = run(&d, &transcript, &pending)
                && request.action.is_destructive()
            {
                prop_assert!(request.p >= 0.95);
                let AgentTarget::Agent(id) = &request.target else {
                    return Err(TestCaseError::fail("destructive on non-agent target"));
                };
                prop_assert!(contains_words(&transcript, id));
            }
        }

        /// Lowering the action probability never turns a non-execute into an execute.
        #[test]
        fn monotonic_in_confidence(d in arb_decision(), pending in arb_pending(), k in 0.0f64..1.0) {
            let hi = run(&d, "cancela migracion auth", &pending);
            let mut lower = d.clone();
            if let Some(a) = lower.action.as_mut() {
                a.p = a.p.map(|p| p * k);
            }
            let lo = run(&lower, "cancela migracion auth", &pending);
            if lo.label() == "execute" {
                prop_assert_eq!(hi.label(), "execute");
            }
        }

        /// Thresholds are always within floors, ordered and ≤ 1.
        #[test]
        fn sanitized_thresholds(e in any::<f64>(), r in any::<f64>(), x in any::<f64>()) {
            let t = Thresholds { execute: e, reversible: r, destructive: x }.sanitized();
            prop_assert!(t.reversible >= FLOORS.reversible && t.reversible <= 1.0);
            prop_assert!(t.execute >= FLOORS.execute && t.execute <= 1.0);
            prop_assert!(t.destructive >= FLOORS.destructive && t.destructive <= 1.0);
            prop_assert!(t.reversible <= t.execute && t.execute <= t.destructive);
        }
    }
}
