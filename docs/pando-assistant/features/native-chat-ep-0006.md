---
created_at: 2026-10-12T08:09:19.774879332Z
updated_at: 2026-10-12T08:09:19.774879332Z
tags:
    - feature
    - chat
    - app
    - ui
---
# Native chat and human-in-the-loop views (PDA-EP-0006, PDA-EP-0007)

Builds on [[pando-assistant/changes/pando-client-chat-model]] (ChatModel, approvals ledger, steering, usage),
[[pando-assistant/changes/pando-client-api-layer]], [[pando-assistant/features/app-shell-us-0002]] (EventPump, ui facade)
and [[pando-assistant/plans/backlog-execution_plan.md]]. Tasks: PDA-T-0064, 0065 (view), 0066, 0067, 0068, 0069, 0070,
0071, 0072 (UI part), 0073 (view), 0074, 0075, 0076, 0077 (view), 0078 (view), 0079, 0080, 0081, 0083.
Branch `epic/PDA-EP-0006-chat` (worktree `../pando-assistant-wt/chat`).

## What changed
New module tree `crates/app/src/chat/`:
- Pure, GPUI-free, unit-tested: `markdown.rs` (pulldown-cmark -> `Block`/`Inline`, unclosed fence streams as code),
  `tools.rs` (`classify`, `arg_summary`, `layout` folding consecutive tool calls into `Row::Activity`, per-type view
  data `exec_view/read_view/search_view/fetch_view/json_view`, base64 data-URL images), `diff.rs` (`diff_lines`,
  `parse_diff` for `{file_path, old_string, new_string}` and `{file_path, new_content}`, `changed_files`, `side_by_side`),
  `sessions.rs` (`SessionsState`: pages of 100, `refresh_head` keeps loaded tail, local search, date buckets, `layout`),
  `composer.rs` (8,000-char limit, slash filter/ranking, `PromptHistory`, `@` mention detection/apply,
  `insert_dictation`, `context_meter` 70 %/90 %), `attention.rs` (`waiting_queue`, `permission_view`,
  `auto_approvable`, `QuestionFlow` stepper, `PendingTally`).
- `service.rs`: `ChatService` on tokio -> one `async_channel` of `ChatMsg` (drained by `EventPump`): sessions pages,
  `open_session` (history, goal, pending replay, model, auto-approve), `send_prompt` (own SSE, tagged with a run id),
  `follow` (wraps `pando_client::follow_session` with begin_local_run/observe commands), steer, stop, outbox
  delivery, models, commands, file search, rename/delete, background poller (sessions head + `/pending` of running
  sessions).
- `panel.rs` + `render*.rs`: `ChatPanel` entity (sidebar, header, transcript, banners, composer, popups, dialogs).
  Wired into `MainView` (field `chat`, chip "Chat/Organismo" in the title bar that also shows "N por decidir").
- Locale files `chat.es.yml` / `chat.en.yml`; new facade exports in `ui/mod.rs` (Textarea, uniform_list, shimmer, ...).
- `ConnectionHub::client()` accessor.

## Behaviour decisions
- Epoch (model generation) tags every session message; own runs additionally carry a **run id**: a late
  `ChatEnded` of a finished run must not abort the run started from orphaned steering text (found with the mock server).
- A follower `HistoryReload` is ignored while the app streams its own run (it would wipe the live transcript).
- Steer answered `409 not_busy`: text is sent as a normal prompt (`start_local_run`, never re-steered).
- Context meter uses prompt + cache read + cache creation tokens over the model's context window.
- Per-session model: `PUT /models/active` is only ever sent with `sessionId`; for a not-yet-created session the choice is
  kept and passed to `chat_stream` (`model` field). The global model is never changed.

## Safety (PDA-SP-0003)
- Approvals are decided only by dialog buttons (`decide_permission` / `send_answers`, `Origin::Explicit`).
  No default button, no Enter/Space shortcut; Escape denies (fail-closed). A dialog takes keyboard focus so typing
  never reaches the composer.
- Auto-approve (header switch / Shift+Tab, optimistic with rollback) calls `Origin::Auto` only for requests that
  `auto_approvable` allows; `require_explicit_approval`, `never_auto_approve` and `execute_unsandboxed` never qualify,
  and the ledger refuses them anyway (tests cover both layers). "Allow for the session" is hidden for flagged ones.
- Switching session or ending a run denies pending items through the ledger and the outbox is delivered.

## GPUI findings worth keeping
- Navigation keys (Up/Down/Tab/Shift+Tab) are consumed by the input's own key bindings before any `key_down`
  listener, even capture-phase ones. Bind own actions with a dynamic key context on an ancestor
  (`ChatPopup > Input`, `ChatHistory > Input`) registered after `ui::init` (`chat/keys.rs`).
- Absolute overlays need `.occlude()` to stop clicks reaching what is below, and `deferred(..)` to paint over
  later siblings. Rows of a `uniform_list` are as wide as their content unless wrapped in a `w_full` element.
- `gen` is a reserved keyword in edition 2024.
- Tests that start tokio work must not receive cross-thread wakeups: use a client pointing at a listener that never
  answers (`hanging_client()` in `chat/tests.rs`).

## Verification
`cargo clippy -p app --all-targets -- -D warnings`, `cargo test -p app` (74 chat tests: pure modules, fixtures through the
real ChatModel, safety properties, steering race, 1,000 sessions, 300-turn transcript + 2,000 deltas, key handling),
`cargo xtask check-deps`, `cargo deny check licenses`.
End to end under xvfb against a scripted mock Pando (`mockpando.py`, not committed): history of a rich session, live
streaming with tools/plan/todos/usage, permission dialog (Escape denial reached the server), questions stepper with
multi-select and review (answers verified server side), steering, per-session model PUT with `sessionId`,
auto-approve POST, slash palette, `@` mention, diff viewer, goal banner, pending jump. Not verified against a real
`pando serve` run with an LLM (no tokens spent).

## Open items
- Real Pando checks: counters vs WebUI (T-0071), IME es/ja in the textarea (T-0074), 50 fps with real GPU (T-0066).
- agentvcs diff source for the changed-files bar and `raw_output.metadata.files_changed` are not used yet.
- Long transcripts are not virtualized (about 110 ms/frame in debug with 900 items).
- The title-bar chip toggles chat/organism; the final layout is the director's call.
