---
id: PDA-SP-0002
type: spec
title: Chat client and sessions
status: backlog
author: mcp
labels: [chat]
created: 2026-10-11T21:41:32Z
updated: 2026-10-12T08:09:46Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/sessions.rs#SessionsState
        - crates/app/src/chat/render.rs#ChatPanel::sidebar
        - crates/app/src/chat/panel.rs#ChatPanel::load_more_sessions
      tests:
        - crates/app/src/chat/sessions.rs#thousand_sessions_layout_is_cheap
        - crates/app/src/chat/tests.rs#a_thousand_sessions_render
        - crates/app/src/chat/tests.rs#the_first_session_event_puts_the_session_on_top
  R2:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/panel.rs#ChatPanel::on_msg
        - crates/app/src/chat/service.rs#ChatService::open_session
      tests:
        - crates/app/src/chat/tests.rs#history_fixture_and_live_events_render
  R3:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/markdown.rs#parse
        - crates/app/src/chat/md_view.rs#blocks_el
        - crates/app/src/chat/service.rs#ChatService::send_prompt
        - crates/app/src/chat/render_transcript.rs#ChatPanel::transcript
      tests:
        - crates/app/src/chat/markdown.rs#an_unclosed_fence_streams_as_code
        - crates/app/src/chat/tests.rs#a_burst_of_events_through_the_channel_is_applied_in_order
        - crates/app/src/chat/tests.rs#follow_reset_drops_only_the_live_run_and_a_long_transcript_streams_smoothly
  R4:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/service.rs#ChatService::follow
        - crates/app/src/chat/panel.rs#ChatPanel::on_msg
      tests:
        - crates/app/src/chat/tests.rs#the_late_end_of_a_finished_run_does_not_kill_the_resent_steering
        - crates/app/src/chat/tests.rs#a_history_reload_never_wipes_the_run_being_streamed
        - crates/app/src/chat/tests.rs#late_messages_of_a_previous_session_are_dropped
  R5:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/tools.rs#layout
        - crates/app/src/chat/diff.rs#changed_files
        - crates/app/src/chat/render_transcript.rs#ChatPanel::tool_card
        - crates/app/src/chat/render_banners.rs#ChatPanel::diff_viewer
      tests:
        - crates/app/src/chat/tools.rs#consecutive_tools_fold_into_one_activity_row
        - crates/app/src/chat/diff.rs#changed_files_aggregate_per_path_in_first_seen_order
        - crates/app/src/chat/diff.rs#edit_diff_counts_changed_lines_only
  R6:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/panel.rs#ChatPanel::submit_text
        - crates/app/src/chat/panel.rs#ChatPanel::stop_with
        - crates/app/src/chat/render_composer.rs#ChatPanel::steering_chips
      tests:
        - crates/app/src/chat/tests.rs#steering_while_running_shows_a_chip_that_survives_the_race
        - crates/app/src/chat/tests.rs#the_late_end_of_a_finished_run_does_not_kill_the_resent_steering
  R7:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/composer.rs#insert_dictation
        - crates/app/src/chat/panel.rs#ChatPanel::insert_dictation
        - crates/app/src/chat/render_composer.rs#ChatPanel::composer_area
        - crates/app/src/chat/keys.rs#init
      tests:
        - crates/app/src/chat/tests.rs#dictation_is_inserted_marked_and_never_sent
        - crates/app/src/chat/tests.rs#the_slash_palette_picks_a_command
        - crates/app/src/chat/tests.rs#picking_a_mention_inserts_the_relative_path
        - crates/app/src/chat/tests.rs#the_model_choice_is_per_session_and_defers_for_new_ones
  R8:
    status: backlog
    trace:
      code:
        - crates/app/src/chat/render_banners.rs#ChatPanel::banners
        - crates/app/src/chat/render_banners.rs#ChatPanel::usage_panel
        - crates/app/src/chat/render_transcript.rs#notice_el
      tests:
        - crates/app/src/chat/tests.rs#banners_dialogs_and_usage_render_together
---

## Purpose
Complete native chat with Pando: sessions, streaming, tools, plan/goal, steering, cancel and run recovery.

## Scope
`POST /api/v1/chat/stream`, `GET /sessions[/{id}]`, `/sessions/{id}/stream` (replay of 500 events), `/steer`, `/cancel`, `/pending`, `/models`, `/commands`, `/goal`. Full SSE parser (§3.3), history mapper for PascalCase / untagged parts, `run_seq` protocol. GPUI UI: session list, transcript, composer.

Refs: `docs/research/pando-api.md` §2.2, §3, §4.1, §7.

## Requirements

### PDA-SP-0002.R1 — Session list

The UI SHALL list sessions from `GET /api/v1/sessions` paginated (100 per page, "load more"), newest first, with title, `prompt_preview`, message count and a running indicator (`is_running`), client-side search, rename (`PATCH`) and delete (`DELETE`, with confirmation). "New" SHALL be lazy: the session is created by the first prompt (`chat/stream` without `sessionId`).

#### Scenario: new conversation
- WHEN the user clicks New and sends a prompt
- THEN the first `session` event's id becomes the selected session and it appears at the top of the list

### PDA-SP-0002.R2 — Tolerant history loading

The client SHALL load history with `GET /api/v1/sessions/{id}`, deserialise PascalCase session/message JSON with `#[serde(default)]`, discriminate untagged parts by keys (text, thinking, tool call, tool result, finish, image url, binary), fold `Role:"tool"` results into their tool calls by id, and render unknown parts as a generic JSON block instead of failing.

#### Scenario: legacy session
- GIVEN a session with tool calls and results in separate messages
- THEN each tool card shows input and output together

### PDA-SP-0002.R3 — Response streaming

The client SHALL send prompts with `POST /api/v1/chat/stream {sessionId?, prompt, model?}` and handle every event in the catalogue (`session, run, thinking_delta, content_delta, tool_call, tool_call_update, tool_result, plan_update, todos_update, token_usage, system_message, steering_*, conclusion_*, resurrected, permission_request, question_request, goal_status, error, done`). Deltas SHALL be applied in batches (one render per drained burst). The final assistant text SHALL be the accumulation of `content_delta` until `done`.

#### Scenario: long answer
- GIVEN 2,000 `content_delta` events in 3 s
- THEN the UI stays ≥ 50 fps and the markdown is complete at `done`

#### Scenario: unclosed code fence mid-stream
- THEN the partial block renders as code without flicker

### PDA-SP-0002.R4 — Reattach and run recovery (run_seq)

The client SHALL implement the `run_seq` protocol: on opening a session set `seenRunSeq = is_running ? -1 : run_seq`; poll `GET /sessions/{id}/pending` every ~4 s; when no stream is open and `run_seq > seenRunSeq`, open `GET /sessions/{id}/stream`; reset the live bubble before applying a replay; reload history when the replay yields no parts. A stream that closes without `done` SHALL trigger a reattach. When Pando exposes a global session-activity stream (gap G5) the client SHALL prefer it over polling.

#### Scenario: subagent result wakes mesnada
- GIVEN the session is idle and a delegated task finishes
- WHEN Pando starts a resurrected run
- THEN within 5 s the client attaches, shows the `resurrected` notice and streams the reply

#### Scenario: app restarted mid-run
- THEN reopening the session replays the buffered events and continues live

### PDA-SP-0002.R5 — Tool cards and diffs

Tool calls SHALL render as cards keyed by `id` with kind, title, status and locations, grouped into collapsible activity groups ("Ejecutó 3 comandos · leyó 2 ficheros"). Renderers SHALL exist for execute (command, output, exit code), edit (inline diff with +/-), write, read (truncated), search/grep/glob, fetch, images and a generic JSON fallback. A file-changes bar SHALL summarise changed files with a diff view resolved from `tool_result.diff` or agentvcs blobs.

#### Scenario: edit tool
- GIVEN a `tool_result` with `diff{file_path, old_string, new_string}`
- THEN the card shows a coloured unified diff and the bar counts +/− lines

### PDA-SP-0002.R6 — Steering and cancellation

While a run is active the composer SHALL send text as steering (`POST /sessions/{id}/steer`, shown as "En cola" chips; on `steering_injected` the assistant bubble closes, the user text is inserted and a new bubble opens). A Stop control SHALL call `POST /sessions/{id}/cancel` and also cancel an active goal.

#### Scenario: steer
- GIVEN a running session
- WHEN the user types "y además añade tests"
- THEN a queued chip appears and becomes a user message on `steering_injected`

### PDA-SP-0002.R7 — Composer: commands, model and dictation

The composer SHALL support Enter to send / Shift+Enter newline, an 8,000-character limit, a slash-command palette from `GET /api/v1/commands`, a model switcher from `GET /api/v1/models` applying `PUT /models/active` with `sessionId` (session scope only), a context meter (warn 70 %, danger 90 %), @file mentions via `/api/v1/files/search`, and insertion of dictated text from the voice pipeline without sending.

#### Scenario: model switch is session-scoped
- WHEN the user picks a model in a session
- THEN the request carries `sessionId` and the global config model is unchanged

### PDA-SP-0002.R8 — Plan, todos, goal, usage and notices

The chat SHALL show a plan banner from `plan_update`, todos from `todos_update`, a goal banner from `goal_status` (objective, status, iteration n/max, next step, cancel via `POST /sessions/{id}/goal/cancel`), a token meter from `token_usage` (estimated vs confirmed, cost, cache and reasoning breakdown) and notice rows for `system_message` routing/context-filter, `conclusion_*` and `resurrected`.

#### Scenario: goal running
- GIVEN `goal_status` with iteration 3/10
- THEN the banner shows 3/10 and a cancel button
