//! Scripted end-to-end spoken conversations (PDA-T-0115): fixtures instead of audio and a real
//! Pando, wired through the same pure parts the app uses (decision engine, executor, command
//! speech, answer accumulator, session machine, latency turn). The live variant (WAV as mic,
//! real Pando, TTS to a file) is `#[ignore]` below.

use std::sync::{Arc, Mutex};

use decide::command::{Capabilities, Dispatched, dispatch};
use decide::policy::{Outcome, Thresholds};
use decide::transport::{ClientOptions, SystemOneClient, block_on};
use organism::BodyState;
use pando_client::api::{CreateTask, Project, TaskResponse};
use pando_client::chat::{ChatEvent, ToolResultEvent};

use super::latency::{Stage, Stats, Turn};
use super::session::{Decided, Input, Output, Session};
use super::speak::{AnswerAccumulator, Fed, speakable};
use crate::decision::directory::AgentDirectory;
use crate::decision::engine::{StateSnapshot, decide_transcript, testing::Scripted};
use crate::decision::executor::{
    ApprovalScope, Backend, PandoExecutor, UiPort, VoiceDecisionReport,
};
use crate::decision::speech::command_speech;

#[derive(Default)]
struct World {
    cancelled: Mutex<Vec<String>>,
}

impl Backend for World {
    async fn tasks(&self) -> Result<Vec<TaskResponse>, String> {
        Ok(vec![TaskResponse {
            id: "t1".into(),
            status: "running".into(),
            progress: 40,
            current_tool: "edit".into(),
            ..TaskResponse::default()
        }])
    }
    async fn cancel(&self, id: &str) -> Result<(), String> {
        self.cancelled.lock().expect("lock").push(id.into());
        Ok(())
    }
    async fn task(&self, _: &str) -> Result<TaskResponse, String> {
        Err("404".into())
    }
    async fn create(&self, _: &CreateTask) -> Result<TaskResponse, String> {
        Err("no".into())
    }
    async fn projects(&self) -> Result<Vec<Project>, String> {
        Ok(vec![])
    }
}

struct NoUi;
impl UiPort for NoUi {
    fn open(&self, _: Option<String>) {}
    async fn decide(&self, _: ApprovalScope, _: bool) -> VoiceDecisionReport {
        VoiceDecisionReport::default()
    }
}

fn snapshot() -> StateSnapshot {
    StateSnapshot {
        agents: AgentDirectory::from_rows([(
            "t1".into(),
            "Migración auth".into(),
            "running".into(),
            None,
        )]),
        projects: vec![],
        current_project: None,
        pending: vec![],
    }
}

fn client(rounds: Vec<Vec<(&'static str, &'static str, f64)>>) -> SystemOneClient<Scripted> {
    SystemOneClient::new(Scripted::new(rounds), ClientOptions::new("tev1:0.8b"))
}

fn body_changes(out: &[Output]) -> Vec<BodyState> {
    out.iter()
        .filter_map(|o| {
            if let Output::SetBody(b) = o {
                Some(*b)
            } else {
                None
            }
        })
        .collect()
}

fn speak_text(out: &[Output]) -> Option<(u64, String)> {
    out.iter().find_map(|o| match o {
        Output::Speak(u) => Some((u.id, u.text.clone())),
        _ => None,
    })
}

#[test]
fn spoken_status_command_is_answered_aloud_within_the_stage_budget() {
    crate::i18n::apply(Some("es"));
    let mut session = Session::new();
    let mut turn = Turn::started(1_000);
    let mut stats = Stats::default();

    assert_eq!(
        body_changes(&session.step(Input::PttPressed)),
        [BodyState::Listening]
    );
    assert_eq!(
        body_changes(&session.step(Input::PttReleased { submitted: true })),
        [BodyState::Thinking]
    );
    turn.mark(Stage::PcmReady, 1_004);
    let out = session.step(Input::Transcript("cómo va migración auth".into()));
    turn.mark(Stage::Transcript, 1_180);
    assert!(matches!(out.as_slice(), [Output::Decide(t)] if t == "cómo va migración auth"));

    let resolved = block_on(decide_transcript(
        &client(vec![
            vec![("intent", "agent_status", 0.97)],
            vec![
                ("agent", "migracion-auth", 0.96),
                ("action", "status", 0.97),
            ],
        ]),
        "cómo va migración auth",
        &snapshot(),
        Thresholds::default(),
        1,
    ));
    turn.mark(Stage::Decision, 1_260);
    let Outcome::Execute { request } = resolved.outcome else {
        panic!("{:?}", resolved.outcome)
    };
    session.step(Input::Decided(Decided::Command));

    let executor = PandoExecutor::new(
        Arc::new(World::default()),
        Arc::new(NoUi),
        snapshot().agents,
    );
    let Dispatched::Ran(result) = block_on(dispatch(
        &executor,
        &request,
        "cómo va migración auth",
        Capabilities::default(),
    )) else {
        panic!("refused")
    };
    turn.mark(Stage::Api, 1_275);
    let speech = command_speech(&snapshot().agents, &result);
    assert!(
        speech.contains("40") && speech.contains("Migración auth"),
        "{speech}"
    );

    let out = session.step(Input::CommandFinished {
        ok: true,
        speech: Some(speech.clone()),
    });
    let (utt, text) = speak_text(&out).expect("spoken");
    assert_eq!(text, speech);
    session.step(Input::SpeechStarted(utt));
    turn.mark(Stage::FirstAudio, 1_420);
    let out = session.step(Input::SpeechFinished(utt));
    assert_eq!(body_changes(&out), [BodyState::Idle]);

    stats.finish(&turn);
    // Release -> first audio stays inside the 1.5 s conversational budget (`docs/02`).
    assert!(turn.total_ms() < 1_500, "{}", turn.summary());
    assert_eq!(stats.report().len(), 5);
}

#[test]
fn a_spoken_cancel_asks_first_and_runs_only_after_a_yes() {
    crate::i18n::apply(Some("es"));
    let world = Arc::new(World::default());
    let resolved = block_on(decide_transcript(
        &client(vec![
            vec![("intent", "stop_agent", 0.97)],
            vec![
                ("agent", "migracion-auth", 0.97),
                ("action", "cancel", 0.97),
            ],
        ]),
        "cancela el de autenticación",
        &snapshot(),
        Thresholds::default(),
        1,
    ));
    let Outcome::Confirm { request, reason } = resolved.outcome else {
        panic!("{:?}", resolved.outcome)
    };
    assert!(
        world.cancelled.lock().expect("lock").is_empty(),
        "nothing ran before the yes"
    );
    let c = crate::decision::confirm::Confirmation::confirm(
        "cancela el de autenticación",
        request,
        reason,
        0,
    );
    // "no": nothing runs.
    let no = crate::decision::confirm::lexical_yes_no("no");
    assert_eq!(
        c.answer(no, 1_000),
        crate::decision::confirm::Verdict::Dismissed
    );
    // silence for 8 s: nothing runs either.
    assert_eq!(
        c.answer(crate::decision::confirm::lexical_yes_no("sí"), 8_000),
        crate::decision::confirm::Verdict::Dismissed
    );
    // "sí" in time: the command runs.
    let crate::decision::confirm::Verdict::Execute(req) =
        c.answer(crate::decision::confirm::lexical_yes_no("sí"), 2_000)
    else {
        panic!("expected execute")
    };
    let executor = PandoExecutor::new(world.clone(), Arc::new(NoUi), snapshot().agents);
    let Dispatched::Ran(r) = block_on(dispatch(&executor, &req, "", Capabilities::default()))
    else {
        panic!("refused")
    };
    assert!(r.is_ok());
    assert_eq!(
        *world.cancelled.lock().expect("lock"),
        vec!["t1".to_owned()]
    );
}

/// The voice side of the UI: applies a spoken approve/deny to a real approvals ledger the way
/// `Assistant::on_ui` does (`spoken_approvals::apply`, which decides with `Origin::Voice`).
struct LedgerUi(Mutex<pando_client::approvals::Approvals>);

impl UiPort for LedgerUi {
    fn open(&self, _: Option<String>) {}
    async fn decide(&self, _: ApprovalScope, approve: bool) -> VoiceDecisionReport {
        use crate::decision::spoken_approvals::{Applied, Spoken, apply};
        let mut ledger = self.0.lock().expect("lock");
        let spoken = if approve {
            Spoken::Approve
        } else {
            Spoken::Deny
        };
        let mut report = VoiceDecisionReport::default();
        for id in ledger.pending_ids() {
            match apply(&mut ledger, &id, &spoken) {
                Applied::Resolved(_) => report.resolved += 1,
                Applied::NeedsScreen => report.needs_explicit += 1,
                Applied::Gone | Applied::Unclear => {}
            }
        }
        report
    }
}

/// ADR 0009 (decision of 2026-10-12, PDA-T-0103): an approval flagged
/// `require_explicit_approval` / `never_auto_approve` is approved only by a click. A spoken
/// "sí" never approves it — not through the policy, not through a voice confirmation, not by
/// answering the read-aloud request.
#[test]
fn a_flagged_approval_is_never_approved_by_a_spoken_yes() {
    use crate::decision::confirm::{Confirmation, Verdict, lexical_yes_no};
    use crate::decision::spoken_approvals::{self, Applied, Spoken};
    use decide::policy::PendingApproval;
    use pando_client::chat::PermissionRequest;

    crate::i18n::apply(Some("es"));
    let flagged_perm = || PermissionRequest {
        id: "p1".into(),
        session_id: "s1".into(),
        tool_name: "bash".into(),
        description: "borrar la rama".into(),
        action: "execute".into(),
        require_explicit_approval: true,
        ..PermissionRequest::default()
    };
    let approve_rounds = |p: f64| {
        client(vec![
            vec![("intent", "answer_permission", 0.99)],
            vec![("agent", "migracion-auth", 0.99), ("action", "approve", p)],
        ])
    };

    // 1. Policy: with the flag known, even p = 0.99 and the literal id give ExplicitOnly.
    let mut snap = snapshot();
    snap.pending = vec![PendingApproval {
        agent_id: "migracion-auth".into(),
        explicit_only: true,
    }];
    let resolved = block_on(decide_transcript(
        &approve_rounds(0.99),
        "sí, aprueba migracion-auth",
        &snap,
        Thresholds::default(),
        1,
    ));
    assert!(
        matches!(resolved.outcome, Outcome::ExplicitOnly { .. }),
        "{:?}",
        resolved.outcome
    );

    // 2. A stale snapshot without the flag asks for a confirmation; the spoken "sí" reaches
    //    the ledger with Origin::Voice, which refuses the flagged item.
    let resolved = block_on(decide_transcript(
        &approve_rounds(0.7),
        "aprueba lo de migración auth",
        &snapshot(),
        Thresholds::default(),
        1,
    ));
    let Outcome::Confirm { request, reason } = resolved.outcome else {
        panic!("{:?}", resolved.outcome)
    };
    let c = Confirmation::confirm("aprueba lo de migración auth", request, reason, 0);
    let Verdict::Execute(req) = c.answer(lexical_yes_no("sí"), 1_000) else {
        panic!("a yes in time runs the command")
    };
    let mut ledger = spoken_approvals::ledger();
    ledger.register_permission(flagged_perm(), std::time::Duration::ZERO);
    let ui = Arc::new(LedgerUi(Mutex::new(ledger)));
    let executor = PandoExecutor::new(
        Arc::new(World::default()),
        ui.clone(),
        AgentDirectory::from_rows([(
            "t1".into(),
            "Migración auth".into(),
            "running".into(),
            Some("s1".into()),
        )]),
    );
    let Dispatched::Ran(result) = block_on(dispatch(&executor, &req, "", Capabilities::default()))
    else {
        panic!("refused before the ledger")
    };
    assert!(result.is_err(), "{result:?}");
    assert!(
        ui.0.lock().expect("lock").is_pending("p1"),
        "the flagged request is still waiting for a click"
    );

    // 3. Read aloud and answered "sí": the ledger refuses it too.
    let mut ledger = spoken_approvals::ledger();
    ledger.register_permission(flagged_perm(), std::time::Duration::ZERO);
    let spoken = spoken_approvals::interpret("sí", 0);
    assert_eq!(spoken, Spoken::Approve);
    assert_eq!(
        spoken_approvals::apply(&mut ledger, "p1", &spoken),
        Applied::NeedsScreen
    );
    assert!(ledger.is_pending("p1"));
}

/// PDA-SP-0008.R8: "háblale a auth" and "vuelve" switch the target with no confirmation and
/// no command; an unknown agent never switches.
#[test]
fn hablale_and_vuelve_switch_the_target_without_confirmation() {
    use decide::policy::VoiceTarget;
    crate::i18n::apply(Some("es"));
    let resolved = block_on(decide_transcript(
        &client(vec![vec![("agent", "migracion-auth", 0.7)]]),
        "háblale a auth",
        &snapshot(),
        Thresholds::default(),
        1,
    ));
    let Outcome::SwitchTarget { target, p } = &resolved.outcome else {
        panic!("{:?}", resolved.outcome)
    };
    assert_eq!(target, &VoiceTarget::Agent("migracion-auth".into()));
    assert!((p - 0.7).abs() < 1e-9);
    assert_eq!(
        snapshot()
            .agents
            .resolve("migracion-auth")
            .map(|e| e.cell_id.as_str()),
        Some("t1")
    );
    let mut session = Session::new();
    session.step(Input::PttPressed);
    session.step(Input::PttReleased { submitted: true });
    session.step(Input::Transcript("háblale a auth".into()));
    let out = session.step(Input::Decided(Decided::SwitchTarget));
    assert!(!out.iter().any(|o| matches!(o, Output::Chat { .. })));

    for text in ["vuelve", "vuelve a Pando", "volver"] {
        let resolved = block_on(decide_transcript(
            &client(vec![]),
            text,
            &snapshot(),
            Thresholds::default(),
            1,
        ));
        assert_eq!(
            resolved.outcome,
            Outcome::SwitchTarget {
                target: VoiceTarget::Primary,
                p: 1.0
            },
            "{text}"
        );
    }

    // Low agent confidence: clarify with the top-2, nothing switches.
    let resolved = block_on(decide_transcript(
        &client(vec![vec![("agent", "migracion-auth", 0.4)]]),
        "habla con el de docs",
        &snapshot(),
        Thresholds::default(),
        1,
    ));
    assert!(
        matches!(
            resolved.outcome,
            Outcome::Conversation {
                reason: decide::policy::ConversationReason::LowConfidence,
                ..
            }
        ),
        "{:?}",
        resolved.outcome
    );
}

#[test]
fn a_conversation_speaks_only_the_final_answer_never_tool_output() {
    crate::i18n::apply(Some("es"));
    let mut session = Session::new();
    session.step(Input::PttPressed);
    session.step(Input::PttReleased { submitted: true });
    session.step(Input::Transcript("hola, ¿puedes mirar el README?".into()));
    session.step(Input::Decided(Decided::Conversation));

    let mut acc = AnswerAccumulator::new();
    let events = [
        ChatEvent::ThinkingDelta("voy a leerlo".into()),
        ChatEvent::ToolResult(ToolResultEvent {
            content: "SECRET=hunter2 contenido del README".into(),
            ..ToolResultEvent::default()
        }),
        ChatEvent::ContentDelta("El README explica ".into()),
        ChatEvent::ContentDelta("cómo instalar la app.".into()),
        ChatEvent::Done,
    ];
    let mut spoken = None;
    for ev in &events {
        match acc.feed(ev) {
            Fed::FirstContent => {
                assert_eq!(
                    body_changes(&session.step(Input::ChatFirstContent)),
                    [BodyState::Idle]
                );
            }
            Fed::Done => {
                let out = session.step(Input::ChatFinished(speakable(acc.text()).map(|s| s.text)));
                spoken = speak_text(&out);
            }
            _ => {}
        }
    }
    let (utt, text) = spoken.expect("spoken");
    assert_eq!(text, "El README explica cómo instalar la app.");
    assert!(!text.contains("hunter2"));
    session.step(Input::SpeechStarted(utt));
    let out = session.step(Input::SpeechFinished(utt));
    assert_eq!(body_changes(&out), [BodyState::Idle]);
}

#[test]
fn barge_in_during_the_answer_stops_audio_and_the_new_prompt_steers_the_run() {
    crate::i18n::apply(Some("es"));
    let mut session = Session::new();
    session.step(Input::PttPressed);
    session.step(Input::PttReleased { submitted: true });
    session.step(Input::Transcript("cuéntame algo largo".into()));
    session.step(Input::Decided(Decided::Conversation));
    session.step(Input::ChatFirstContent);
    let out = session.step(Input::ChatFinished(Some("Érase una vez...".into())));
    let (utt, _) = speak_text(&out).expect("spoken");
    session.step(Input::SpeechStarted(utt));
    let started = std::time::Instant::now();
    let out = session.step(Input::PttPressed);
    assert!(out.contains(&Output::StopAudio));
    // The state machine's part of the < 50 ms budget is negligible; the audio stop is measured in the audio crate.
    assert!(started.elapsed() < std::time::Duration::from_millis(50));
    assert_eq!(body_changes(&out), [BodyState::Listening]);
}

#[test]
#[ignore = "needs a real Pando, an STT model and the pando-stt/pando-tts workers; run by hand"]
fn live_wav_to_speech_report() {
    // Manual script (see scripts/wer-gate.sh for how the worker is driven): inject a WAV as the
    // microphone, run the decision against the real decision model and Pando, synthesize the
    // answer to a file sink and print `Stats::report()`. Left as a placeholder because it needs
    // credentials and hardware that CI and this machine do not have.
}
