---
created_at: 2026-10-12T09:01:19.933486964Z
updated_at: 2026-10-12T09:01:19.933486964Z
tags:
    - fix
---
# Post-merge stabilization (organism/shell + chat + voice)

Builds on [[pando-assistant/plans/backlog-execution_plan.md]].

## What changed
- GPUI tests that run a real tokio runtime now call `cx.executor().allow_parking()` (root cause of `main_view_renders_the_empty_state_without_pando` and the flaky `a_history_reload_never_wipes_the_run_being_streamed`: tokio workers wake GPUI tasks, e.g. when a channel Sender is dropped at runtime shutdown or a request fails, and the deterministic TestScheduler rejects wakeups from foreign threads). Runtimes are shut down with `shutdown_timeout`.
- One push-to-talk control: the 60 px mic circle of the voice dock was removed; the orb chrome pill (`cells/chrome.rs::mic_button`, T-0117) owns it and sends `Command::Ptt{Toggle}` to the shell, whose outputs reach `voice::controller::ptt_edge` via `voice::assistant::shell_output` (app.rs). The dock F9 key also goes through the shell when it runs (`shell::runtime::is_running`).
- pando-client TLS tests name the aws-lc-rs provider (rcgen enables ring too, so rustls cannot pick a default).
- Traces added for PDA-SP-0003.R5, SP-0005.R6, SP-0006.R1-R8, SP-0007.R1-R6, SP-0011.R1-R4 (tasks themselves link no requirements).

## Files
crates/app/src/{views/main_view.rs, chat/tests.rs, cells/pane.rs, voice/controller.rs, shell/runtime.rs, app.rs}, crates/pando-client/tests/common/mod.rs, design-system/README.md.

## Gap found
PDA-T-0171: voice entities live in MainView, so global PTT/hands-free do nothing in background mode and tray-opened settings lack Voice/Audio sections.

## Verification
`cargo test -p app` x6 and the app test binary x60 all green; `cargo test --workspace --no-fail-fast`, fmt, clippy -D warnings, check-deps, tokens --check, deny licenses OK; xvfb `--smoke-test` and `--demo` screenshot OK.
