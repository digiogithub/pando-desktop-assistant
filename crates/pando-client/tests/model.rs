//! ChatModel: snapshot tests over every recorded fixture plus behaviour tests.
//! Refresh snapshots with `UPDATE_SNAPSHOTS=1 cargo test -p pando-client --test model`.

use std::sync::Arc;
use std::time::Duration;

use pando_client::approvals::{Decision, DenyReason, ManualClock, Origin, Refused, Resolution};
use pando_client::chat::{Goal, Note, PermissionRequest};
use pando_client::model::{ItemKind, NoticeKind, ToolStatus};
use pando_client::steering::SendPlan;
use pando_client::{ChatEvent, ChatModel, SessionDetail, SseParser};

fn events(raw: &[u8]) -> Vec<ChatEvent> {
    let mut p = SseParser::new();
    let mut out = p.push(raw);
    out.extend(p.finish());
    out.iter().map(ChatEvent::from_sse).collect()
}

fn model() -> ChatModel {
    ChatModel::new(Arc::new(ManualClock::new()))
}

fn snapshot(name: &str, actual: &str) {
    let path = format!("{}/tests/snapshots/{name}.txt", env!("CARGO_MANIFEST_DIR"));
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, actual).unwrap_or_else(|e| panic!("{e}"));
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing snapshot {path}: {e} (run with UPDATE_SNAPSHOTS=1)"));
    assert_eq!(actual, expected, "snapshot {name} differs");
}

fn run_fixture(name: &str, raw: &[u8]) {
    let mut m = model();
    for e in events(raw) {
        m.apply(&e);
    }
    snapshot(name, &m.render_debug());
}

#[test]
fn snapshot_chat_simple() {
    run_fixture("chat_simple", include_bytes!("fixtures/chat_simple.sse"));
}

#[test]
fn snapshot_chat_tool_read() {
    run_fixture(
        "chat_tool_read",
        include_bytes!("fixtures/chat_tool_read.sse"),
    );
}

#[test]
fn snapshot_chat_slash() {
    run_fixture("chat_slash", include_bytes!("fixtures/chat_slash.sse"));
}

#[test]
fn snapshot_chat_goal_status() {
    run_fixture(
        "chat_goal_status",
        include_bytes!("fixtures/chat_goal_status.sse"),
    );
}

#[test]
fn snapshot_session_reattach() {
    run_fixture(
        "session_reattach",
        include_bytes!("fixtures/session_reattach.sse"),
    );
}

#[test]
fn snapshot_synthetic_events() {
    run_fixture(
        "synthetic_events",
        include_bytes!("fixtures/synthetic_events.sse"),
    );
}

#[test]
fn snapshot_history() {
    let d: SessionDetail = serde_json::from_str(include_str!("fixtures/session_detail.json"))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut m = model();
    m.load_history(&d);
    snapshot("history", &m.render_debug());
    assert_eq!(m.seen_run_seq(), 1);
    assert!(!m.is_running());
    assert!(
        m.usage().current().is_some_and(|u| u.cache_read_tokens > 0),
        "session totals loaded"
    );
}

#[test]
fn live_tool_call_folds_its_result_and_measures_duration() {
    let clock = Arc::new(ManualClock::new());
    let mut m = ChatModel::new(clock.clone());
    for e in events(include_bytes!("fixtures/synthetic_events.sse"))
        .iter()
        .take(8)
    {
        m.apply(e);
        clock.advance(Duration::from_millis(250));
    }
    let tool = m.items().iter().find_map(|i| match &i.kind {
        ItemKind::Tool(t) => Some(t),
        _ => None,
    });
    let t = tool.unwrap_or_else(|| panic!("no tool"));
    assert_eq!(t.status, ToolStatus::Completed);
    assert!(t.outcome.as_ref().is_some_and(|o| o.diff.is_some()));
    assert!(t.duration.is_some());
}

#[test]
fn replay_after_reset_equals_a_single_application() {
    let raw = include_bytes!("fixtures/session_reattach.sse");
    let mut once = model();
    for e in events(raw) {
        once.apply(&e);
    }
    let mut twice = model();
    for e in events(raw) {
        twice.apply(&e);
    }
    // The follower resets the live bubble before every replay.
    twice.reset_live();
    for e in events(raw) {
        twice.apply(&e);
    }
    let strip = |s: String| {
        s.lines()
            .filter(|l| !l.starts_with("-- running"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(strip(once.render_debug()), strip(twice.render_debug()));
}

#[test]
fn replay_keeps_the_user_message_of_a_local_run() {
    let mut m = model();
    assert_eq!(m.submit("hello"), SendPlan::Prompt("hello".into()));
    for e in events(include_bytes!("fixtures/chat_simple.sse")) {
        m.apply(&e);
    }
    m.reset_live();
    // The cut happens when the replay's `Run` event shows it is the same run again.
    for e in events(include_bytes!("fixtures/chat_simple.sse"))
        .iter()
        .take(2)
    {
        m.apply(e);
    }
    assert_eq!(m.items().len(), 1);
    assert!(matches!(&m.items()[0].kind, ItemKind::User { text, .. } if text == "hello"));
}

#[test]
fn late_events_of_a_previous_session_are_dropped() {
    let mut m = model();
    let g1 = m.begin_session(Some("a"));
    assert!(m.apply_gen(g1, &ChatEvent::ContentDelta("x".into())));
    let g2 = m.begin_session(Some("b"));
    assert_ne!(g1, g2);
    assert!(!m.apply_gen(g1, &ChatEvent::ContentDelta("late".into())));
    assert!(m.items().is_empty());
    assert_eq!(m.session_id(), Some("b"));
}

#[test]
fn revision_changes_on_every_event() {
    let mut m = model();
    let r0 = m.revision();
    m.apply(&ChatEvent::ContentDelta("a".into()));
    assert!(m.revision() > r0);
}

#[test]
fn steering_flow_chip_then_injection_closes_the_bubble() {
    let mut m = model();
    m.submit("start");
    m.apply(&ChatEvent::ContentDelta("working".into()));
    let SendPlan::Steer { chip, .. } = m.submit("also docs") else {
        panic!("expected steering")
    };
    m.steering_mut().steer_accepted(chip);
    assert_eq!(m.steering().chips().len(), 1);
    m.apply(&ChatEvent::SteeringInjected(Note {
        session_id: "s".into(),
        message: "also docs".into(),
    }));
    assert!(m.steering().chips().is_empty());
    m.apply(&ChatEvent::ContentDelta("more".into()));
    let kinds: Vec<_> = m.items().iter().map(|i| &i.kind).collect();
    assert!(
        matches!(
            kinds[1],
            ItemKind::Assistant {
                streaming: false,
                ..
            }
        ),
        "bubble closed"
    );
    assert!(matches!(kinds[2], ItemKind::User { steered: true, text } if text == "also docs"));
    assert!(
        matches!(kinds[3], ItemKind::Assistant { streaming: true, text } if text == "more"),
        "new bubble"
    );
}

#[test]
fn steer_done_race_returns_the_text() {
    let mut m = model();
    m.submit("start");
    let SendPlan::Steer { chip, .. } = m.submit("lost?") else {
        panic!("expected steering")
    };
    m.steering_mut().steer_accepted(chip);
    m.apply(&ChatEvent::Done);
    assert!(!m.is_running());
    assert_eq!(m.take_orphaned_steering(), vec!["lost?"]);
    assert!(m.take_orphaned_steering().is_empty());
    assert!(m.steering().chips().is_empty());
}

#[test]
fn approvals_fail_closed_inside_the_model() {
    let mut m = model();
    m.apply(&ChatEvent::Session {
        session_id: "s".into(),
        running: true,
    });
    m.apply(&ChatEvent::PermissionRequest(PermissionRequest {
        id: "p1".into(),
        session_id: "s".into(),
        tool_name: "bash".into(),
        never_auto_approve: true,
        ..PermissionRequest::default()
    }));
    m.apply(&ChatEvent::PermissionRequest(PermissionRequest {
        id: "p2".into(),
        session_id: "s".into(),
        ..PermissionRequest::default()
    }));
    // Replays do not duplicate rows.
    m.apply(&ChatEvent::PermissionRequest(PermissionRequest {
        id: "p1".into(),
        session_id: "s".into(),
        ..PermissionRequest::default()
    }));
    assert_eq!(
        m.items()
            .iter()
            .filter(|i| matches!(i.kind, ItemKind::Approval { .. }))
            .count(),
        2
    );
    let card = m.approval_card("p1").unwrap_or_else(|| panic!("card"));
    assert!(card.pending && card.explicit_only);
    assert_eq!(
        m.decide("p1", Decision::Approve, Origin::Voice),
        Err(Refused::NeedsExplicit)
    );
    assert_eq!(
        m.decide("p2", Decision::Approve, Origin::Explicit),
        Ok(Resolution::Approved)
    );
    // The run ends with p1 unanswered: denied, never approved.
    m.apply(&ChatEvent::Done);
    assert_eq!(
        m.approvals().resolution("p1"),
        Some(&Resolution::Denied(DenyReason::ServerExpired))
    );
    assert!(m.approval_card("p1").is_some_and(|c| !c.pending));
}

#[test]
fn approval_deadlines_use_the_injected_clock() {
    let clock = Arc::new(ManualClock::new());
    let mut m = ChatModel::new(clock.clone());
    m.apply(&ChatEvent::PermissionRequest(PermissionRequest {
        id: "p".into(),
        session_id: "s".into(),
        ..PermissionRequest::default()
    }));
    assert!(m.expire_approvals().is_empty());
    clock.advance(Duration::from_secs(121));
    assert_eq!(m.expire_approvals(), vec!["p"]);
    assert_eq!(
        m.decide("p", Decision::Approve, Origin::Explicit),
        Err(Refused::AlreadyResolved)
    );
}

#[test]
fn session_switch_denies_pending_and_keeps_the_denials_deliverable() {
    let mut m = model();
    m.apply(&ChatEvent::PermissionRequest(PermissionRequest {
        id: "p".into(),
        session_id: "s".into(),
        ..PermissionRequest::default()
    }));
    m.begin_session(Some("other"));
    assert!(m.items().is_empty());
    assert_eq!(
        m.approvals().resolution("p"),
        Some(&Resolution::Denied(DenyReason::SessionSwitch))
    );
    assert_eq!(m.approvals().outbox().len(), 1);
}

#[test]
fn cancelled_goal_updates_the_banner() {
    let mut m = model();
    let g = |status: &str| Goal {
        objective: "ship".into(),
        status: status.into(),
        iteration: 2,
        max_iterations: 5,
        ..Goal::default()
    };
    m.apply(&ChatEvent::GoalStatus(Some(g("running"))));
    let b = m.goal_banner().unwrap_or_else(|| panic!("banner"));
    assert!(b.can_cancel && (b.iteration, b.max_iterations) == (2, 5));
    assert!(m.stop_plan().cancel_goal || !m.is_running());
    m.apply(&ChatEvent::GoalStatus(Some(g("cancelled"))));
    let b = m.goal_banner().unwrap_or_else(|| panic!("banner"));
    assert!(!b.can_cancel && b.status == "cancelled");
    m.apply(&ChatEvent::GoalStatus(None));
    assert!(m.goal_banner().is_none());
}

#[test]
fn plan_and_todos_persist_after_the_run() {
    let mut m = model();
    for e in events(include_bytes!("fixtures/synthetic_events.sse")) {
        m.apply(&e);
    }
    assert!(!m.is_running());
    assert_eq!(m.plan().len(), 2);
    assert_eq!(m.plan()[0].active_form.as_deref(), Some("Writing tests"));
    assert_eq!(m.todos()[0].priority, "high");
}

#[test]
fn usage_keeps_every_field_through_the_model() {
    let mut m = model();
    for e in events(include_bytes!("fixtures/chat_tool_read.sse")) {
        m.apply(&e);
    }
    let u = m.usage().totals().unwrap_or_else(|| panic!("usage"));
    assert_eq!(
        (u.prompt_tokens, u.completion_tokens, u.cache_read_tokens),
        (61, 36761, 36750)
    );
    assert_eq!(u.context_window, 400_000);
    assert!(!m.usage().is_estimated());
}

#[test]
fn history_maps_conclusions_finish_reasons_and_unknown_parts() {
    let json = r#"{"session":{"ID":"s","Title":"T"},"is_running":true,"run_seq":4,"messages":[
      {"ID":"1","Role":"user","Parts":[{"text":"do it"}]},
      {"ID":"2","Role":"assistant","Parts":[{"text":"ok"},{"mystery":1},{"url":"http://x/i.png"},{"reason":"max_tokens","time":1}]},
      {"ID":"3","Role":"user","Parts":[{"text":"[delegated-result task=t1 engine=claude project=demo status=success confidence=0.90]\nsummary: Fixed it\nfollow_up: ship"}]},
      {"ID":"4","Role":"assistant","Parts":[{"reason":"canceled","time":2}]}]}"#;
    let d: SessionDetail = serde_json::from_str(json).unwrap_or_else(|e| panic!("{e}"));
    let mut m = model();
    m.load_history(&d);
    assert_eq!(m.title(), "T");
    assert!(m.is_running());
    assert_eq!(
        m.seen_run_seq(),
        -1,
        "running session: attach on the first poll"
    );
    let k: Vec<_> = m.items().iter().map(|i| &i.kind).collect();
    assert!(matches!(k[0], ItemKind::User { .. }));
    assert!(matches!(k[2], ItemKind::Json { .. }));
    assert!(matches!(k[3], ItemKind::Image { .. }));
    assert!(matches!(
        k[4],
        ItemKind::Notice {
            kind: NoticeKind::MaxTokens,
            ..
        }
    ));
    assert!(matches!(k[5], ItemKind::Conclusion(c) if c.task == "t1" && c.summary == "Fixed it"));
    assert!(matches!(
        k[6],
        ItemKind::Notice {
            kind: NoticeKind::Cancelled,
            ..
        }
    ));
}

#[test]
fn unknown_events_are_shown_not_lost() {
    let mut m = model();
    m.apply(&ChatEvent::Unknown {
        event: "future".into(),
        raw: r#"{"a":1}"#.into(),
    });
    assert!(matches!(&m.items()[0].kind, ItemKind::Json { label, .. } if label == "future"));
}

#[test]
fn snapshot_chat_permission() {
    run_fixture(
        "chat_permission",
        include_bytes!("fixtures/chat_permission.sse"),
    );
}

#[test]
fn snapshot_chat_permission_deny() {
    run_fixture(
        "chat_permission_deny",
        include_bytes!("fixtures/chat_permission_deny.sse"),
    );
}

#[test]
fn snapshot_chat_question_steer() {
    run_fixture(
        "chat_question_steer",
        include_bytes!("fixtures/chat_question_steer.sse"),
    );
}

#[test]
fn snapshot_chat_cancel() {
    run_fixture("chat_cancel", include_bytes!("fixtures/chat_cancel.sse"));
}

#[test]
fn snapshot_chat_cancel_while_permission() {
    run_fixture(
        "chat_cancel_while_permission",
        include_bytes!("fixtures/chat_cancel_while_permission.sse"),
    );
}

#[test]
fn snapshot_chat_delegate_spawn() {
    run_fixture(
        "chat_delegate_spawn",
        include_bytes!("fixtures/chat_delegate_spawn.sse"),
    );
}

#[test]
fn snapshot_chat_conclusion_injected() {
    run_fixture(
        "chat_conclusion_injected",
        include_bytes!("fixtures/chat_conclusion_injected.sse"),
    );
}

#[test]
fn snapshot_chat_edit_diff() {
    run_fixture(
        "chat_edit_diff",
        include_bytes!("fixtures/chat_edit_diff.sse"),
    );
}

#[test]
fn snapshot_session_resurrected() {
    run_fixture(
        "session_resurrected",
        include_bytes!("fixtures/session_resurrected.sse"),
    );
}

#[test]
fn a_resurrected_run_keeps_the_rows_of_the_finished_run() {
    let mut m = model();
    for e in events(include_bytes!("fixtures/chat_delegate_spawn.sse")) {
        m.apply(&e);
    }
    assert!(!m.is_running());
    m.reset_live();
    for e in events(include_bytes!("fixtures/session_resurrected.sse")) {
        m.apply(&e);
    }
    let after = m.render_debug();
    assert!(
        after.contains("tool mesnada_spawn_agent"),
        "run 1 rows dropped:\n{after}"
    );
    assert!(after.contains("Resuming"), "{after}");
}
