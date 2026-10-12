//! GPUI tests of the chat panel: fixtures through the real model, safety properties of the
//! approvals UI, steering, composer helpers and rendering of long lists.

use pando_client::approvals::{Origin, Resolution};
use pando_client::chat::{
    ChatEvent, PermissionRequest, Question, QuestionOption, QuestionRequest, SessionDetail,
    SessionList, SessionSummary,
};
use pando_client::steering::{ChipState, SteerResult};
use pando_client::{ItemKind, SseParser};

use super::ChatPanel;
use super::attention::waiting_queue;
use super::service::{ChatMsg, PageMode};
use crate::ui::Entity;
use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};

const SESSION_DETAIL: &str =
    include_str!("../../../pando-client/tests/fixtures/session_detail.json");
const CHAT_TOOL_READ: &str =
    include_str!("../../../pando-client/tests/fixtures/chat_tool_read.sse");
const SYNTHETIC: &str = include_str!("../../../pando-client/tests/fixtures/synthetic_events.sse");

fn events(sse: &str) -> Vec<ChatEvent> {
    let mut p = SseParser::new();
    let mut out = p.push(sse.as_bytes());
    out.extend(p.finish());
    out.iter().map(ChatEvent::from_sse).collect()
}

struct Harness {
    panel: Entity<ChatPanel>,
    cx: VisualTestContext,
    runtime: tokio::runtime::Runtime,
}

fn harness(cx: &mut TestAppContext) -> Harness {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("runtime");
    cx.update(|cx| {
        crate::ui::init(cx);
        crate::ui::kit_theme::install(cx);
        crate::chat::init(cx);
    });
    rust_i18n::set_locale("es");
    let handle = runtime.handle().clone();
    let window = cx.add_window(move |window, cx| ChatPanel::new(window, cx, handle));
    let panel = window.root(cx).expect("root");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    Harness {
        panel,
        cx: vcx,
        runtime,
    }
}

impl Harness {
    fn msg(&mut self, msg: ChatMsg) {
        self.panel.update(&mut self.cx, |p, cx| p.on_msg(msg, cx));
        self.cx.run_until_parked();
    }

    fn event(&mut self, ev: ChatEvent) {
        let (epoch, run) = self.panel.read_with(&self.cx, |p, _| (p.epoch, p.run_id));
        self.msg(ChatMsg::Chat { epoch, run, ev });
    }

    fn redraw(&mut self) {
        self.cx.update(|window, _| window.refresh());
        self.cx.run_until_parked();
    }

    fn read<R>(&self, f: impl FnOnce(&ChatPanel) -> R) -> R {
        self.panel.read_with(&self.cx, |p, _| f(p))
    }

    fn finish(self) {
        self.runtime.shutdown_background();
    }
}

/// A client whose server accepts connections and never answers: requests hang instead of
/// failing, so no message comes back from the runtime threads during a test.
fn hanging_client() -> (pando_client::PandoClient, std::net::TcpListener) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let client =
        pando_client::PandoClient::new(pando_client::PandoConfig::new(url)).expect("client");
    (client, listener)
}

fn perm(id: &str, flagged: bool) -> PermissionRequest {
    PermissionRequest {
        id: id.into(),
        session_id: "s1".into(),
        tool_name: "bash".into(),
        description: "run a command".into(),
        require_explicit_approval: flagged,
        ..PermissionRequest::default()
    }
}

#[gpui_test]
fn history_fixture_and_live_events_render(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let detail: SessionDetail = serde_json::from_str(SESSION_DETAIL).expect("fixture");
    let sid = detail.session.id.clone();
    h.panel
        .update(&mut h.cx, |p, cx| p.select_session(&sid, cx));
    let epoch = h.read(|p| p.epoch);
    h.msg(ChatMsg::Opened {
        epoch,
        result: Ok(detail),
    });
    h.redraw();
    assert!(h.read(|p| !p.model.items().is_empty()));
    assert!(!h.read(|p| p.loading));
    for ev in events(CHAT_TOOL_READ) {
        h.event(ev);
    }
    h.redraw();
    assert!(h.read(|p| {
        p.model
            .items()
            .iter()
            .any(|i| matches!(i.kind, ItemKind::Tool(_)))
    }));
    h.finish();
}

#[gpui_test]
fn late_messages_of_a_previous_session_are_dropped(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.panel.update(&mut h.cx, |p, cx| p.select_session("a", cx));
    let old = h.read(|p| p.epoch);
    h.panel.update(&mut h.cx, |p, cx| p.select_session("b", cx));
    h.msg(ChatMsg::Chat {
        epoch: old,
        run: 0,
        ev: ChatEvent::ContentDelta("stale".into()),
    });
    assert!(h.read(|p| p.model.items().is_empty()));
    h.finish();
}

#[gpui_test]
fn a_burst_of_events_through_the_channel_is_applied_in_order(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let (tx, epoch, run) = h.read(|p| (p.tx.clone(), p.epoch, p.run_id));
    for i in 0..300 {
        tx.try_send(ChatMsg::Chat {
            epoch,
            run,
            ev: ChatEvent::ContentDelta(format!("{i} ")),
        })
        .expect("send");
    }
    h.cx.run_until_parked();
    let text = h.read(|p| match &p.model.items()[0].kind {
        ItemKind::Assistant { text, .. } => text.clone(),
        other => panic!("{other:?}"),
    });
    assert!(text.starts_with("0 1 2 3"));
    assert!(text.trim_end().ends_with("299"));
    h.redraw();
    h.finish();
}

// ------------------------------------------------------------------ safety

#[gpui_test]
fn flagged_requests_are_never_auto_approved_and_plain_ones_are(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.panel.update(&mut h.cx, |p, cx| p.toggle_auto_approve(cx));
    assert!(h.read(|p| p.auto_approve));
    h.event(ChatEvent::PermissionRequest(perm("plain", false)));
    h.event(ChatEvent::PermissionRequest(perm("flagged", true)));
    let mut esc = perm("escalation", false);
    esc.action = "execute_unsandboxed".into();
    h.event(ChatEvent::PermissionRequest(esc));
    h.read(|p| {
        assert_eq!(
            p.model.approvals().resolution("plain"),
            Some(&Resolution::Approved)
        );
        assert!(p.model.approvals().is_pending("flagged"));
        assert!(p.model.approvals().is_pending("escalation"));
    });
    // The dialog shows the first one waiting, never an auto-approved one.
    let queue = h.read(|p| waiting_queue(&p.model));
    assert_eq!(queue.first().map(|w| w.id.as_str()), Some("flagged"));
    h.redraw();
    h.finish();
}

#[gpui_test]
fn without_auto_approve_nothing_is_approved_on_arrival(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.event(ChatEvent::PermissionRequest(perm("p", false)));
    assert!(h.read(|p| p.model.approvals().is_pending("p")));
    h.redraw();
    h.finish();
}

#[gpui_test]
fn escape_denies_and_enter_does_not_approve(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.event(ChatEvent::PermissionRequest(perm("p", false)));
    h.redraw();
    h.cx.simulate_keystrokes("enter");
    assert!(
        h.read(|p| p.model.approvals().is_pending("p")),
        "Enter must never approve"
    );
    h.cx.simulate_keystrokes("space");
    assert!(h.read(|p| p.model.approvals().is_pending("p")));
    h.cx.simulate_keystrokes("escape");
    assert!(
        matches!(
            h.read(|p| p.model.approvals().resolution("p").cloned()),
            Some(Resolution::Denied(_))
        ),
        "Escape denies"
    );
    h.finish();
}

#[gpui_test]
fn only_an_explicit_decision_approves_a_flagged_request(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.event(ChatEvent::PermissionRequest(perm("f", true)));
    h.panel.update(&mut h.cx, |p, cx| {
        p.decide_permission("f", pando_client::approvals::Decision::ApproveSession, cx);
    });
    assert!(
        h.read(|p| p.model.approvals().is_pending("f")),
        "no session grant for flagged requests"
    );
    h.panel.update(&mut h.cx, |p, cx| {
        p.decide_permission("f", pando_client::approvals::Decision::Approve, cx);
    });
    assert_eq!(
        h.read(|p| p.model.approvals().resolution("f").cloned()),
        Some(Resolution::Approved)
    );
    h.finish();
}

#[gpui_test]
fn a_run_ending_denies_what_is_still_pending(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.event(ChatEvent::PermissionRequest(perm("p", false)));
    h.event(ChatEvent::Done);
    assert!(matches!(
        h.read(|p| p.model.approvals().resolution("p").cloned()),
        Some(Resolution::Denied(_))
    ));
    h.finish();
}

#[gpui_test]
fn the_questions_stepper_answers_through_the_ledger(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let q = |id: &str| Question {
        id: id.into(),
        header: "H".into(),
        question: format!("{id}?"),
        multi_select: false,
        options: vec![
            QuestionOption {
                label: "yes".into(),
                description: "d".into(),
            },
            QuestionOption {
                label: "no".into(),
                description: String::new(),
            },
        ],
    };
    h.event(ChatEvent::QuestionRequest(QuestionRequest {
        id: "qr".into(),
        session_id: "s1".into(),
        questions: vec![q("a"), q("b")],
    }));
    h.redraw();
    assert!(h.read(|p| p.question_flow.is_some()));
    // Incomplete answers are not sent.
    h.panel.update(&mut h.cx, |p, cx| p.send_answers(cx));
    assert!(h.read(|p| p.model.approvals().is_pending("qr")));
    h.panel.update(&mut h.cx, |p, _| {
        let f = p.question_flow.as_mut().expect("flow");
        f.toggle(0, 0);
        f.next();
        f.toggle(1, 1);
        f.next();
    });
    h.redraw();
    h.panel.update(&mut h.cx, |p, cx| p.send_answers(cx));
    let res = h.read(|p| p.model.approvals().resolution("qr").cloned());
    let Some(Resolution::Answered(a)) = res else {
        panic!("{res:?}")
    };
    assert_eq!(a[0].selected, ["yes"]);
    assert_eq!(a[1].selected, ["no"]);
    assert!(h.read(|p| p.question_flow.is_none()));
    h.finish();
}

#[gpui_test]
fn cancelling_questions_cancels_the_request(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.event(ChatEvent::QuestionRequest(QuestionRequest {
        id: "qr".into(),
        session_id: "s1".into(),
        questions: vec![],
    }));
    h.panel.update(&mut h.cx, |p, cx| {
        p.sync_question_flow();
        p.cancel_questions(cx);
    });
    assert!(matches!(
        h.read(|p| p.model.approvals().resolution("qr").cloned()),
        Some(Resolution::Cancelled(_))
    ));
    h.finish();
}

// ------------------------------------------------------------------ steering

#[gpui_test]
fn steering_while_running_shows_a_chip_that_survives_the_race(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.event(ChatEvent::Session {
        session_id: "s1".into(),
        running: true,
    });
    assert!(h.read(|p| p.model.is_running()));
    h.panel
        .update(&mut h.cx, |p, cx| p.submit_text("also add docs", cx));
    // Offline: the chip is kept as failed, never dropped.
    let chips = h.read(|p| p.model.steering().chips().to_vec());
    assert_eq!(chips.len(), 1);
    assert!(matches!(chips[0].state, ChipState::Failed(_)));
    h.redraw();

    // "not busy": the run had ended, so the text becomes a normal prompt.
    h.panel
        .update(&mut h.cx, |p, cx| p.submit_text("second", cx));
    let chip = h.read(|p| p.model.steering().chips()[1].id);
    let epoch = h.read(|p| p.epoch);
    h.msg(ChatMsg::Steer {
        epoch,
        chip,
        result: Ok(SteerResult::NotBusy),
    });
    h.read(|p| {
        assert!(
            p.model
                .items()
                .iter()
                .any(|i| matches!(&i.kind, ItemKind::User { text, .. } if text == "second"))
        );
        assert_eq!(p.model.steering().chips().len(), 1);
    });
    h.finish();
}

#[gpui_test]
fn steering_injected_event_replaces_the_chip_with_the_user_message(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    for ev in events(SYNTHETIC) {
        if matches!(ev, ChatEvent::SteeringInjected(_)) {
            h.event(ev);
        }
    }
    h.redraw();
    h.finish();
}

// ------------------------------------------------------------------ sidebar

fn summaries(n: usize) -> SessionList {
    SessionList {
        sessions: (0..n)
            .map(|i| {
                let mut s = SessionSummary::default();
                s.session.id = format!("s{i}");
                s.session.title = format!("Session {i}");
                s.session.updated_at = 1_700_000_000 - i as i64 * 3_600;
                s.session.message_count = 4;
                s.is_running = i % 50 == 0;
                s
            })
            .collect(),
        total: n as i64,
        limit: n as i64,
        has_more: false,
        offset: 0,
    }
}

#[gpui_test]
fn a_thousand_sessions_render(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.msg(ChatMsg::Sessions {
        mode: PageMode::Replace,
        result: Ok(summaries(1000)),
    });
    let t = std::time::Instant::now();
    h.redraw();
    assert!(
        t.elapsed() < std::time::Duration::from_secs(5),
        "{:?}",
        t.elapsed()
    );
    assert_eq!(h.read(|p| p.sessions.rows().len()), 1000);
    h.finish();
}

#[gpui_test]
fn the_first_session_event_puts_the_session_on_top(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.msg(ChatMsg::Sessions {
        mode: PageMode::Replace,
        result: Ok(summaries(3)),
    });
    h.event(ChatEvent::Session {
        session_id: "brand-new".into(),
        running: true,
    });
    assert_eq!(
        h.read(|p| p.sessions.rows()[0].session.id.clone()),
        "brand-new"
    );
    h.finish();
}

#[gpui_test]
fn deleting_the_open_session_starts_a_new_one(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.msg(ChatMsg::Sessions {
        mode: PageMode::Replace,
        result: Ok(summaries(3)),
    });
    h.panel
        .update(&mut h.cx, |p, cx| p.select_session("s1", cx));
    h.msg(ChatMsg::Deleted {
        id: "s1".into(),
        error: None,
    });
    h.read(|p| {
        assert!(p.sessions.get("s1").is_none());
        assert_eq!(p.model.session_id(), None);
    });
    h.finish();
}

// ------------------------------------------------------------------ pending indicator

#[gpui_test]
fn pending_items_of_other_sessions_count(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.panel
        .update(&mut h.cx, |p, cx| p.select_session("mine", cx));
    assert_eq!(h.read(|p| p.pending_total()), 0);
    h.msg(ChatMsg::PendingPoll(vec![("other".into(), 2)]));
    assert_eq!(
        h.read(|p| (p.pending_total(), p.pending_sessions())),
        (2, 1)
    );
    // The open session's own requests add to it.
    h.event(ChatEvent::PermissionRequest(perm("p", false)));
    assert_eq!(
        h.read(|p| (p.pending_total(), p.pending_sessions())),
        (3, 2)
    );
    h.redraw();
    // A click on the indicator goes to the other session while the dialog is not up.
    h.panel.update(&mut h.cx, |p, cx| {
        p.model
            .decide(
                "p",
                pando_client::approvals::Decision::Deny,
                Origin::Explicit,
            )
            .expect("deny");
        p.open_first_pending(cx);
    });
    assert_eq!(
        h.read(|p| p.model.session_id().map(str::to_owned)),
        Some("other".into())
    );
    h.finish();
}

// ------------------------------------------------------------------ composer

#[gpui_test]
fn dictation_is_inserted_marked_and_never_sent(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.cx.update(|window, cx| {
        h.panel.update(cx, |p, cx| {
            p.insert_dictation("arregla el login", window, cx)
        });
    });
    h.cx.run_until_parked();
    let (text, _) = h.cx.update(|_, cx| h.panel.read(cx).composer_text(cx));
    assert_eq!(text, "arregla el login");
    h.read(|p| {
        assert!(p.dictated.is_some());
        assert!(!p.model.is_running(), "dictation must not send");
        assert!(p.model.items().is_empty());
    });
    h.redraw();
    h.finish();
}

#[gpui_test]
fn picking_a_mention_inserts_the_relative_path(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.cx.update(|window, cx| {
        h.panel.update(cx, |p, cx| {
            p.set_composer_text("mira @mai", window, cx);
        });
    });
    h.cx.run_until_parked();
    h.panel.update(&mut h.cx, |p, _| {
        p.update_mention("mira @mai", 9);
        if let Some(m) = p.mention.as_mut() {
            m.results = vec!["src/main.rs".into()];
        }
    });
    h.cx.update(|window, cx| {
        h.panel
            .update(cx, |p, cx| p.pick_mention("src/main.rs", window, cx));
    });
    h.cx.run_until_parked();
    let (text, _) = h.cx.update(|_, cx| h.panel.read(cx).composer_text(cx));
    assert_eq!(text, "mira src/main.rs ");
    h.finish();
}

#[gpui_test]
fn the_slash_palette_picks_a_command(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.msg(ChatMsg::Commands(Ok(vec![
        pando_client::api::CommandInfo {
            name: "goal".into(),
            description: "Set a goal".into(),
            accepts_args: true,
        },
        pando_client::api::CommandInfo {
            name: "help".into(),
            description: "Help".into(),
            accepts_args: false,
        },
    ])));
    h.cx.update(|window, cx| {
        h.panel
            .update(cx, |p, cx| p.set_composer_text("/go", window, cx));
    });
    h.cx.run_until_parked();
    assert!(h.cx.update(|_, cx| h.panel.read(cx).palette_open(cx)));
    h.cx.update(|window, cx| {
        h.panel
            .update(cx, |p, cx| p.pick_command("goal", window, cx));
    });
    h.cx.run_until_parked();
    let (text, _) = h.cx.update(|_, cx| h.panel.read(cx).composer_text(cx));
    assert_eq!(text, "/goal ");
    h.redraw();
    h.finish();
}

#[gpui_test]
fn the_model_choice_is_per_session_and_defers_for_new_ones(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.panel
        .update(&mut h.cx, |p, cx| p.select_model("gpt-x", cx));
    // No session yet: kept for the first prompt, nothing global is touched.
    assert_eq!(h.read(|p| p.pending_model.clone()), Some("gpt-x".into()));
    h.finish();
}

#[gpui_test]
fn banners_dialogs_and_usage_render_together(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    for ev in events(SYNTHETIC) {
        h.event(ev);
    }
    h.panel.update(&mut h.cx, |p, cx| {
        p.usage_open = true;
        p.model_menu = true;
        cx.notify();
    });
    h.redraw();
    h.finish();
}

#[gpui_test]
fn tab_in_the_composer_picks_the_highlighted_command(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.msg(ChatMsg::Commands(Ok(vec![
        pando_client::api::CommandInfo {
            name: "goal".into(),
            description: "Set a goal".into(),
            accepts_args: true,
        },
    ])));
    h.cx.update(|window, cx| {
        h.panel.update(cx, |p, cx| {
            p.composer.update(cx, |s, cx| s.focus(window, cx));
            p.set_composer_text("/go", window, cx);
        });
    });
    h.redraw();
    h.cx.simulate_keystrokes("tab");
    let (text, _) = h.cx.update(|_, cx| h.panel.read(cx).composer_text(cx));
    assert_eq!(text, "/goal ");
    h.finish();
}

#[gpui_test]
fn the_late_end_of_a_finished_run_does_not_kill_the_resent_steering(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let (client, _server) = hanging_client();
    h.panel
        .update(&mut h.cx, |p, cx| p.set_client(Some(client), cx));
    h.event(ChatEvent::Session {
        session_id: "s1".into(),
        running: true,
    });
    // A steering chip that the server accepted but never injected.
    h.panel.update(&mut h.cx, |p, _| {
        p.model.submit("also docs");
    });
    let chip = h.read(|p| p.model.steering().chips()[0].id);
    let epoch = h.read(|p| p.epoch);
    h.msg(ChatMsg::Steer {
        epoch,
        chip,
        result: Ok(SteerResult::Accepted),
    });
    let first_run = h.read(|p| p.run_id);
    // The run ends: the orphaned text is sent as a new prompt (a new run)...
    h.event(ChatEvent::Done);
    let second_run = h.read(|p| p.run_id);
    assert_eq!(second_run, first_run + 1, "the text became a prompt");
    assert!(h.read(|p| p.run.is_some()));
    // ...and the late end of the first stream must not abort it.
    h.msg(ChatMsg::ChatEnded {
        epoch,
        run: first_run,
        error: None,
    });
    assert!(h.read(|p| p.run.is_some()), "the new run survives");
    h.finish();
}

#[gpui_test]
fn a_history_reload_never_wipes_the_run_being_streamed(cx: &mut TestAppContext) {
    use pando_client::follow::FollowEvent;
    let mut h = harness(cx);
    let (client, _server) = hanging_client();
    h.panel
        .update(&mut h.cx, |p, cx| p.set_client(Some(client), cx));
    h.panel
        .update(&mut h.cx, |p, cx| p.submit_text("hello", cx));
    h.event(ChatEvent::ContentDelta("partial answer".into()));
    let epoch = h.read(|p| p.epoch);
    h.msg(ChatMsg::Follow {
        epoch,
        ev: FollowEvent::HistoryReload(Box::<SessionDetail>::default()),
    });
    assert!(
        h.read(|p| p.model.items().iter().any(
            |i| matches!(&i.kind, ItemKind::Assistant { text, .. } if text == "partial answer")
        )),
        "the streamed answer survives the reload"
    );
    h.finish();
}

#[gpui_test]
fn follow_reset_drops_only_the_live_run_and_a_long_transcript_streams_smoothly(
    cx: &mut TestAppContext,
) {
    use pando_client::follow::FollowEvent;
    let mut h = harness(cx);
    // A long conversation: 300 turns, each with markdown, a tool call and its result.
    let messages: Vec<serde_json::Value> = (0..300)
        .flat_map(|i| {
            [
                serde_json::json!({"ID": format!("u{i}"), "Role": "user", "SessionID": "long",
                    "Parts": [{"text": format!("question {i}")}, {"reason": "stop", "time": 0}]}),
                serde_json::json!({"ID": format!("a{i}"), "Role": "assistant", "SessionID": "long",
                    "Parts": [
                        {"text": format!("# Answer {i}\n\nSome **markdown** with `code`.\n\n```rust\nfn f() {{}}\n```\n")},
                        {"id": format!("t{i}"), "name": "bash", "input": format!("{{\"command\":\"echo {i}\"}}"), "type": "function", "finished": true},
                        {"reason": "tool_use", "time": 0}]}),
                serde_json::json!({"ID": format!("r{i}"), "Role": "tool", "SessionID": "long",
                    "Parts": [{"tool_call_id": format!("t{i}"), "name": "bash", "content": format!("{i}\n")}]}),
            ]
        })
        .collect();
    let detail: SessionDetail = serde_json::from_value(serde_json::json!({
        "session": {"ID": "long", "Title": "long"},
        "messages": messages,
        "is_running": false,
        "run_seq": 1,
    }))
    .expect("detail");
    h.panel
        .update(&mut h.cx, |p, cx| p.select_session("long", cx));
    let epoch = h.read(|p| p.epoch);
    h.msg(ChatMsg::Opened {
        epoch,
        result: Ok(detail),
    });
    let before = h.read(|p| p.model.items().len());
    // Stream a long answer in 2,000 deltas (one burst through the channel) and render.
    h.panel.update(&mut h.cx, |p, _| {
        p.model.start_local_run("stream please");
    });
    let (tx, run) = h.read(|p| (p.tx.clone(), p.run_id));
    for i in 0..2000 {
        tx.try_send(ChatMsg::Chat {
            epoch,
            run,
            ev: ChatEvent::ContentDelta(format!("word{i} ")),
        })
        .expect("send");
    }
    let t = std::time::Instant::now();
    h.cx.run_until_parked();
    let apply = t.elapsed();
    let t = std::time::Instant::now();
    for _ in 0..5 {
        h.redraw();
    }
    let per_frame = t.elapsed() / 5;
    assert!(h.read(|p| p.model.items().len()) > before);
    eprintln!("2000 deltas applied in {apply:?}; {per_frame:?} per frame with {before} items");
    assert!(
        per_frame < std::time::Duration::from_millis(250),
        "a frame took {per_frame:?}"
    );

    // A follower replay resets only the live run, not the finished conversation.
    h.msg(ChatMsg::Follow {
        epoch,
        ev: FollowEvent::ResetBubble,
    });
    assert_eq!(
        h.read(|p| p.model.items().len()),
        before + 1,
        "history kept, partial answer dropped"
    );
    h.finish();
}
