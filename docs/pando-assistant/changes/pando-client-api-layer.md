---
created_at: 2026-10-11T22:40:10.357591834Z
updated_at: 2026-10-11T22:40:10.357591834Z
tags:
    - change
    - pando-client
---
# pando-client API layer: streams, chat, orchestrator, events, conclusions, run_seq

Builds on [[pando-assistant/changes/pando-client-core]] and [[pando-assistant/plans/backlog-execution_plan.md]]. Tasks: PDA-T-0049, 0050, 0051, 0052, 0053, 0054, 0072 (client side).

## What changed
- `stream.rs` (T-0049): `EventStream<T>` self-reconnecting SSE feed (1 to 30 s backoff, Content-Type check, optional idle watchdog, `Last-Event-ID` when ids exist). `PandoClient::with_path_prefix` and `open_sse_with` added to the core.
- `chat.rs` (T-0050): PascalCase `Session`/`Message`, key-discriminated `Part`, `fold_messages`, `ChatEvent` (all events plus `Unknown`), `ChatStream`, and all session/chat calls.
- `api.rs` (T-0051): `TaskResponse` with the optional G2 fields (old servers parse), `TaskTracker` for 1-2 s polling (G1: no push stream), instances, projects, `project_client(id)` proxy router, models, commands, version, files search.
- `events.rs` (T-0052): notification and config event streams, decision-model config/health, `watch_decision_model` (reloads on `decisionModel`/`modelAutoMode`/full-reload events and on reconnect).
- `conclusion.rs` (T-0053): `<pando:conclusion>` parser (port of `parse.go`, no YAML dependency) and `[delegated-result]` reader.
- `follow.rs` (T-0072): `run_seq` protocol (`seen_for`, 4 s `/pending` poll, claim then attach, bubble reset, empty-replay history reload, reattach on close without `done`). UI wiring is left to the app crate; the G5 global stream is the future replacement for polling.
- Tests (T-0054): `tests/{event_stream,chat_fixtures,api,events,follow,conformance,live}.rs`; SSE re-encoding of all real fixtures is byte-identical.

## Verification
`cargo clippy -p pando-client --all-targets -- -D warnings`, `cargo test -p pando-client` (all green, repeated), `cargo xtask check-deps`; live `#[ignore]` tests (discovery, token, ConnectionService, real chat, run_seq reattach) passed against `pando serve` v1.3.0. No CI added (forbidden for now).
