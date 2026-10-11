---
id: PDA-SP-0002
type: spec
title: Chat client and sessions
status: backlog
author: mcp
labels: [chat]
created: 2026-10-11T21:41:32Z
updated: 2026-10-11T22:02:50Z
requirements:
  R1:
    status: backlog
  R2:
    status: backlog
  R3:
    status: backlog
  R4:
    status: backlog
  R5:
    status: backlog
  R6:
    status: backlog
  R7:
    status: backlog
  R8:
    status: backlog
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
