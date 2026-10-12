---
created_at: 2026-10-12T08:46:43.412964212Z
updated_at: 2026-10-12T08:46:43.412964212Z
tags:
    - changes
    - voice
    - decide
    - claude-bridge
---
# Voice app wiring (EP-0008): supervisor, PTT, decision layer, orchestrator, Claude cells, hands-free

Builds on [[pando-assistant/plans/backlog-execution_plan.md]], [[pando-assistant/changes/voice-workers-implementation]], [[pando-assistant/changes/decide-decision-layer]], [[pando-assistant/changes/claude-bridge-hooks]]. Branch `epic/PDA-EP-0008-voice-app` (worktree `pando-assistant-wt/voice`), commits per story.

## What changed
Everything lives in new modules of `crates/app` (plus small additive edits in shared files).

### `crates/app/src/voice/`
- `supervisor.rs` (PDA-T-0090): `Supervisor` thread per worker (`pando-stt`, `pando-tts`), `Launcher`/`Link` abstraction, Hello version+role check, Load*, Ready, `Backoff` (0.5 s..30 s, Failed after 6), requests refused unless Ready, `locate_worker` (exe dir, target dir, PATH).
- `resolve.rs`: models from config, the `models` crate store, the packaged `<exe>/../lib/pando-assistant/models`, vendor dev copy.
- `levels.rs` (T-0085): gate + attack/release smoothing, `OrganismLevels::tick` -> `OrganismView::set_levels`. `ptt.rs` (T-0094): tap < 250 ms ignored, 120 s limit, 300 ms pre-roll, barge-in flag. `devices.rs` (T-0086): error classification, deep links, choices. `latency.rs`/`wer.rs` (T-0096): per-stage P50/P95, WER + gate (local `scripts/wer-gate.sh`, `crates/app/tests/wer_gate.rs`, not CI).
- `service.rs`: capture + playback + both supervisors, streaming 750 ms chunks to STT, hands-free `Audio` blocks, barge-in policy, echo cancel feature. `dock.rs` pure dock model; `controller.rs` GPUI entity (dock render, mic button, F9 hold, settings panel, confirmation buttons); `settings_panel.rs`, `sections.rs` (registers `voice`/`audio` sections), `firstrun.rs` (mic probe, hooks installer).
- `speak.rs` (T-0112): answer accumulator (content_delta only), `speakable`, conclusions, delegated results, Claude Stop speech. `session.rs` (T-0113/0114/0140): pure state machine of the body table in docs/06. `assistant.rs`: orchestrator entity (decision, commands, chat run, speech queue, confirmations, spoken approvals, project pill, "dile a ..."). `e2e.rs` (T-0115): scripted spoken conversations. `vad.rs`: energy VAD for barge-in.

### `crates/app/src/decision/`
`transport.rs` reqwest `DecisionTransport` (config from `/config/decision-model`, bearer from env, redacted, 400 ms, 4 MiB cap; T-0097), `executor.rs` command executor over `PandoClient` + `UiPort` (T-0101), `engine.rs` cascade+policy+log record+card (fail-open), `confirm.rs` (T-0102, 8 s, fail-closed), `spoken_approvals.rs` (T-0103), `health.rs` chip (T-0099), `directory.rs` slugs, `speech.rs`, `eval_cli.rs` (`pando-assistant decisions eval|compare`, T-0106).

### `crates/app/src/claude/`
`mod.rs` bridge entity (HookServer on tokio, SessionTracker, cells, avisame speech, send over socket), `extras.rs` card block, `tell.rs` "dile a <sesion> que ..." (T-0132/0134).

### Shared edits
`cells/pane.rs` voice port (`voice_rows/pending/ledger/client/chat_session`), external cells + extras; `cells/source.rs` external cells; `cells/card.rs` `CellKind::External`; `views/main_view.rs` creates dock, assistant, bridge, chips; `app.rs` shell outputs -> voice; `firstrun` providers + `HooksInstaller::install`; `crates/audio` `Playback::set_duck`; app features `pipewire`, `echo-cancel`.

## Safety
Approvals only through the pane ledger with `Origin::Voice`; flagged requests and sandbox escalations never by voice (`spoken_approvals::needs_screen`); destructive commands need `decide` policy (p >= 0.95 + literal id) or a spoken/clicked yes within 8 s; decision failure falls open to conversation; no secrets logged.

## Verification
`cargo clippy -p app --all-targets -D warnings`, `cargo test -p app` (342 pass), `cargo xtask check-deps`, wer_gate against the real Whistle worker, GUI smoke under xvfb (dock renders, hands-free indicator, hook POST accepted by the in-app receiver). Not verified: real microphone/speaker, real worker kill from the UI, real Claude Code firing hooks, live Pando conversation with TTS, PipeWire build (libs absent), echo cancellation quality.

## Findings
- decide policy executes a destructive command without confirmation when the transcript literally names the agent and p >= 0.95 (spec R2).
- The chat panel test `a_history_reload_never_wipes_the_run_being_streamed` is flaky under the GPUI test scheduler (tokio thread activity), unrelated to this work.
- The pane already has a pill mic button (shell chrome) next to the dock's 60 px mic button.
