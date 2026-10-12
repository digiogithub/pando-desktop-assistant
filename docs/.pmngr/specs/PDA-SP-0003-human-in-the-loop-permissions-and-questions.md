---
id: PDA-SP-0003
type: spec
title: "Human-in-the-loop: permissions and questions"
status: backlog
author: mcp
labels: [chat, safety]
created: 2026-10-11T21:41:32Z
updated: 2026-10-12T08:09:56Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/attention.rs#permission_view
        - crates/app/src/chat/attention.rs#waiting_queue
        - crates/app/src/chat/render_dialogs.rs#ChatPanel::permission_dialog
      tests:
        - crates/app/src/chat/attention.rs#queue_keeps_arrival_order_and_drops_resolved
        - crates/app/src/chat/attention.rs#escalation_view_exposes_command_justification_and_scope
        - crates/app/src/chat/tests.rs#escape_denies_and_enter_does_not_approve
  R2:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/attention.rs#QuestionFlow
        - crates/app/src/chat/render_dialogs.rs#ChatPanel::questions_dialog
        - crates/app/src/chat/panel.rs#ChatPanel::send_answers
      tests:
        - crates/app/src/chat/attention.rs#stepper_requires_an_answer_then_reviews
        - crates/app/src/chat/attention.rs#other_needs_text_and_excludes_options_in_single_select
        - crates/app/src/chat/tests.rs#the_questions_stepper_answers_through_the_ledger
        - crates/app/src/chat/tests.rs#cancelling_questions_cancels_the_request
  R3:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/panel.rs#ChatPanel::decide_permission
        - crates/app/src/chat/panel.rs#ChatPanel::flush_outbox
        - crates/app/src/chat/render.rs#ChatPanel::on_key
      tests:
        - crates/app/src/chat/tests.rs#only_an_explicit_decision_approves_a_flagged_request
        - crates/app/src/chat/tests.rs#a_run_ending_denies_what_is_still_pending
        - crates/app/src/chat/tests.rs#escape_denies_and_enter_does_not_approve
        - crates/app/src/chat/attention.rs#the_ledger_still_refuses_auto_and_voice_for_flagged_requests
  R4:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/attention.rs#auto_approvable
        - crates/app/src/chat/panel.rs#ChatPanel::toggle_auto_approve
        - crates/app/src/chat/panel.rs#ChatPanel::apply_auto_approve
        - crates/app/src/chat/keys.rs#ToggleAutoApprove
      tests:
        - crates/app/src/chat/tests.rs#flagged_requests_are_never_auto_approved_and_plain_ones_are
        - crates/app/src/chat/tests.rs#without_auto_approve_nothing_is_approved_on_arrival
        - crates/app/src/chat/attention.rs#flagged_requests_are_never_auto_approvable
  R5:
    status: backlog
  R6:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/attention.rs#PendingTally
        - crates/app/src/chat/panel.rs#ChatPanel::pending_total
        - crates/app/src/chat/render_dialogs.rs#ChatPanel::pending_indicator
        - crates/app/src/views/main_view.rs#MainView::view_toggle
      tests:
        - crates/app/src/chat/tests.rs#pending_items_of_other_sessions_count
        - crates/app/src/chat/attention.rs#tally_counts_unselected_sessions_and_jumps_to_them
---

## Purpose
The user approves, denies or answers what agents ask for — with mouse, keyboard or voice — without anything being approved by accident.

## Scope
Events `permission_request` / `question_request`, `/sessions/{id}/pending`, `/permissions/respond`, `/questions/respond`, `/auto-approve`. Fail-closed approval ledger (`PendingApprovals` pattern from Bitácora). Voice mapping from the decision layer.

## Requirements

### PDA-SP-0003.R1 — Permission dialog with queue

The UI SHALL show `permission_request`s in a queue (first request + "+N en cola") with tool name, description, path and params, offering Permitir / Permitir en sesión / Denegar mapped to `POST /api/v1/permissions/respond {id, sessionId, action: allow|allow_session|deny}`. When `action == "execute_unsandboxed"` it SHALL use an escalation variant showing the command, `justification` and the grant scope from `grant_key`.

#### Scenario: escalation
- GIVEN a request with `action: execute_unsandboxed`, `grant_key: prefix:cargo test`
- THEN the dialog shows the command, the justification and "Concede: prefijo `cargo test`"

### PDA-SP-0003.R2 — Question dialog (AskUserQuestion)

The UI SHALL render `question_request` as a stepper over its questions with single or multi select options (label + description), a free-text "Otro", a review step and cancel, answering via `POST /api/v1/questions/respond {id, sessionId, answers:[{questionId, selected, otherText}], cancelled?}`.

#### Scenario: multi-select
- GIVEN a question with `multi_select: true`
- WHEN the user ticks two options and confirms
- THEN `selected` contains both labels

### PDA-SP-0003.R3 — Fail-closed

Only an explicit user decision SHALL approve. Pending items SHALL be denied (never approved) on window/panel close, session switch, app quit, connection loss with expired server state, or a local timeout; no answer SHALL be delivered after close. Requests flagged `require_explicit_approval` or `never_auto_approve` SHALL never be approved by auto-approve, voice-confidence shortcuts or "remember" rules.

#### Scenario: quit with pending permission
- GIVEN a pending permission
- WHEN the user quits
- THEN a deny is sent before exit (best effort) and nothing is approved

#### Scenario: never_auto_approve by voice
- GIVEN `never_auto_approve: true`
- WHEN the user says "aprueba" with p = 0.99
- THEN the UI still requires a click/keyboard confirmation

### PDA-SP-0003.R4 — Per-session auto-approve

The UI SHALL show and toggle auto-approve per session (`GET/POST /api/v1/sessions/{id}/auto-approve`, Shift+Tab shortcut) with an optimistic update rolled back on error, and SHALL make the enabled state clearly visible in the header.

#### Scenario: toggle fails
- WHEN the POST fails
- THEN the switch reverts and a toast explains why

### PDA-SP-0003.R5 — Voice response to pending items

The voice pipeline SHALL be able to approve, deny or answer the pending item of the target agent ("aprueba", "deniega", "la segunda opción"), reading the item aloud first. `deny` follows the destructive-action confirmation policy of the decision layer; `approve` SHALL require p ≥ 0.85 and SHALL show the item on screen while confirming.

#### Scenario: approve by voice
- GIVEN a single pending permission on mesnada
- WHEN the user says "aprueba"
- THEN the assistant says "Aprobado: <tool>" and sends `allow`

### PDA-SP-0003.R6 — Global pending indicator

The app SHALL show a global count of agents waiting for the user (header, tray icon, mini window badge), computed from `/sessions/{id}/pending` of the sessions it follows and, when available, from a cross-session pending endpoint (gap G12). Affected cells SHALL show status Waiting.

#### Scenario: background session waiting
- GIVEN a non-selected session raises a permission
- THEN the badge increments and an OS notification is shown if the window is unfocused
