//! Chat/session types against the recorded (and synthetic) fixtures.

use pando_client::chat::{Pending, SessionList};
use pando_client::{ChatEvent, FoldedItem, Part, SessionDetail, SseParser, fold_messages};

fn events(raw: &[u8]) -> Vec<ChatEvent> {
    let mut p = SseParser::new();
    let mut out: Vec<_> = p.push(raw);
    out.extend(p.finish());
    out.iter().map(ChatEvent::from_sse).collect()
}

fn no_unknown(name: &str, evs: &[ChatEvent]) {
    for e in evs {
        assert!(
            !matches!(e, ChatEvent::Unknown { .. }),
            "{name}: unexpected {e:?}"
        );
    }
}

#[test]
fn all_real_chat_fixtures_parse_without_unknown_events() {
    for (name, raw) in [
        ("simple", &include_bytes!("fixtures/chat_simple.sse")[..]),
        ("tool", &include_bytes!("fixtures/chat_tool_read.sse")[..]),
        ("slash", &include_bytes!("fixtures/chat_slash.sse")[..]),
        ("goal", &include_bytes!("fixtures/chat_goal_status.sse")[..]),
        (
            "chat_permission",
            &include_bytes!("fixtures/chat_permission.sse")[..],
        ),
        (
            "chat_permission_deny",
            &include_bytes!("fixtures/chat_permission_deny.sse")[..],
        ),
        (
            "chat_question_steer",
            &include_bytes!("fixtures/chat_question_steer.sse")[..],
        ),
        (
            "chat_cancel",
            &include_bytes!("fixtures/chat_cancel.sse")[..],
        ),
        (
            "chat_cancel_while_permission",
            &include_bytes!("fixtures/chat_cancel_while_permission.sse")[..],
        ),
        (
            "chat_delegate_spawn",
            &include_bytes!("fixtures/chat_delegate_spawn.sse")[..],
        ),
        (
            "chat_conclusion_injected",
            &include_bytes!("fixtures/chat_conclusion_injected.sse")[..],
        ),
        (
            "chat_edit_diff",
            &include_bytes!("fixtures/chat_edit_diff.sse")[..],
        ),
        (
            "session_resurrected",
            &include_bytes!("fixtures/session_resurrected.sse")[..],
        ),
        (
            "reattach",
            &include_bytes!("fixtures/session_reattach.sse")[..],
        ),
    ] {
        let evs = events(raw);
        assert!(!evs.is_empty(), "{name}");
        no_unknown(name, &evs);
    }
}

#[test]
fn simple_chat_accumulates_the_answer() {
    let evs = events(include_bytes!("fixtures/chat_simple.sse"));
    assert!(matches!(&evs[0], ChatEvent::Session { running: true, .. }));
    assert!(matches!(&evs[1], ChatEvent::Run { run_seq: 1, .. }));
    let text: String = evs
        .iter()
        .filter_map(|e| match e {
            ChatEvent::ContentDelta(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "pong");
    assert!(evs.contains(&ChatEvent::Done));
    let usage = evs.iter().find_map(|e| match e {
        ChatEvent::TokenUsage(u) => Some(u),
        _ => None,
    });
    assert!(usage.is_some_and(|u| u.prompt_tokens > 0 && !u.estimated));
}

#[test]
fn slash_goal_stream_has_no_done() {
    let evs = events(include_bytes!("fixtures/chat_goal_status.sse"));
    assert!(matches!(&evs[1], ChatEvent::GoalStatus(None)));
    assert!(matches!(
        evs.last(),
        Some(ChatEvent::Session { running: false, .. })
    ));
    assert!(!evs.contains(&ChatEvent::Done));
}

#[test]
fn synthetic_events_cover_the_rest_of_the_catalogue() {
    let evs = events(include_bytes!("fixtures/synthetic_events.sse"));
    let unknown: Vec<_> = evs
        .iter()
        .filter(|e| matches!(e, ChatEvent::Unknown { .. }))
        .collect();
    assert_eq!(unknown.len(), 1, "only some_future_event: {unknown:?}");
    assert!(evs.iter().any(|e| matches!(e, ChatEvent::PermissionRequest(p) if p.never_auto_approve && p.grant_key.as_deref() == Some("prefix:rm -rf"))));
    assert!(
        evs.iter().any(
            |e| matches!(e, ChatEvent::QuestionRequest(q) if q.questions[0].options.len() == 2)
        )
    );
    assert!(
        evs.iter()
            .any(|e| matches!(e, ChatEvent::PlanUpdate(p) if p.len() == 2))
    );
    assert!(
        evs.iter()
            .any(|e| matches!(e, ChatEvent::SteeringInjected(_)))
    );
    assert!(evs.iter().any(|e| matches!(e, ChatEvent::Resurrected(_))));
    assert!(
        evs.iter()
            .any(|e| matches!(e, ChatEvent::GoalStatus(Some(g)) if g.max_iterations == 5))
    );
    assert!(
        evs.iter()
            .any(|e| matches!(e, ChatEvent::Error(m) if m == "cancelled"))
    );
}

#[test]
fn malformed_known_event_degrades_to_unknown() {
    let ev = pando_client::SseEvent {
        event: "tool_call".into(),
        data: "not json".into(),
        id: None,
    };
    assert!(matches!(
        ChatEvent::from_sse(&ev),
        ChatEvent::Unknown { .. }
    ));
}

#[test]
fn session_detail_parses_and_folds_tool_results_into_calls() {
    let d: SessionDetail = serde_json::from_str(include_str!("fixtures/session_detail.json"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(d.run_seq, 1);
    assert!(!d.is_running);
    assert_eq!(d.messages.len(), 6);
    assert!(matches!(d.messages[0].parts[0], Part::Text(_)));
    assert!(matches!(d.messages[1].parts[0], Part::ToolCall(_)));
    assert!(matches!(d.messages[2].parts[0], Part::ToolResult(_)));
    assert!(matches!(d.messages[1].parts[1], Part::Finish { .. }));

    let folded = fold_messages(&d.messages);
    assert_eq!(folded.len(), 4, "tool messages are dropped");
    let tools: Vec<_> = folded
        .iter()
        .flat_map(|m| &m.items)
        .filter_map(|i| match i {
            FoldedItem::Tool { call, result } => Some((call, result)),
            _ => None,
        })
        .collect();
    assert_eq!(tools.len(), 2);
    for (call, result) in tools {
        assert_eq!(
            result.as_ref().map(|r| r.tool_call_id.as_str()),
            Some(call.id.as_str())
        );
    }
    assert!(
        matches!(folded.last().and_then(|m| m.items.first()), Some(FoldedItem::Text(t)) if t.contains("fixture"))
    );
}

#[test]
fn unknown_part_is_preserved() {
    let p: Part = serde_json::from_str(r#"{"weird":1}"#).unwrap_or_else(|e| panic!("{e}"));
    assert!(matches!(p, Part::Other(_)));
}

#[test]
fn lists_and_pending_parse() {
    let l: SessionList = serde_json::from_str(include_str!("fixtures/sessions_list.json"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(!l.sessions.is_empty());
    assert!(l.sessions[0].session.id.len() > 8);
    assert!(l.sessions[0].run_seq >= 1);
    let p: Pending = serde_json::from_str(include_str!("fixtures/session_pending.json"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(!p.running && p.run_seq == 1);
}

#[test]
fn live_permission_stream_carries_a_pending_request() {
    let evs = events(include_bytes!("fixtures/chat_permission.sse"));
    let req = evs.iter().find_map(|e| match e {
        ChatEvent::PermissionRequest(p) => Some(p),
        _ => None,
    });
    let p = req.expect("permission_request in the live capture");
    assert_eq!(p.tool_name, "bash");
    assert!(!p.require_explicit_approval && !p.never_auto_approve);
    assert!(evs.contains(&ChatEvent::Done));
}

#[test]
fn live_question_stream_has_steering_injected() {
    let evs = events(include_bytes!("fixtures/chat_question_steer.sse"));
    assert!(
        evs.iter()
            .any(|e| matches!(e, ChatEvent::QuestionRequest(_)))
    );
    assert!(
        evs.iter()
            .any(|e| matches!(e, ChatEvent::SteeringInjected { .. }))
    );
}

#[test]
fn live_cancel_ends_with_error_then_done() {
    let evs = events(include_bytes!("fixtures/chat_cancel.sse"));
    let n = evs.len();
    assert!(matches!(&evs[n - 3], ChatEvent::Error(_)));
    assert!(evs.contains(&ChatEvent::Done));
}

#[test]
fn live_delegation_notices_are_typed() {
    let evs = events(include_bytes!("fixtures/session_resurrected.sse"));
    assert!(
        evs.iter()
            .any(|e| matches!(e, ChatEvent::Resurrected { .. }))
    );
    let evs = events(include_bytes!("fixtures/chat_conclusion_injected.sse"));
    assert!(
        evs.iter()
            .any(|e| matches!(e, ChatEvent::ConclusionInjected { .. }))
    );
}
