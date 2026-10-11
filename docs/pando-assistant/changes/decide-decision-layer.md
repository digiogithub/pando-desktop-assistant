---
created_at: 2026-10-11T23:03:39.596121097Z
updated_at: 2026-10-11T23:03:39.596121097Z
tags:
    - pando-assistant
    - change
    - decide
    - safety
---
# decide: System One decision layer (PDA-T-0097/0098/0100/0101/0105/0106)

Serves [[pando-assistant/plans/backlog-execution_plan.md]]; spec PDA-SP-0008 (R1–R7); design in [[docs/03-decision-layer.md]] and [[docs/research/pando-api.md]] §5.

## What changed
Crate `crates/decide` went from a skeleton to the full pure decision layer (deps: serde, serde_json with `float_roundtrip`, thiserror; dev: proptest, tempfile). No network/tokio (check-deps OK).

- `protocol.rs` — `/v1/systemone` wire types mirroring Pando `internal/llm/systemone/client.go`: ordered `Question.criteria`, `Request.questions` as ordered vec encoded via serde_json Map; validation (1–64 questions, 2–26 unique criteria, 64 KiB); `Response::decode_checked` (= checkAnswers); `Answer::p_top()` returns None for invalid probabilities.
- `error.rs` — `DecisionError` kinds as Pando's errors.go; `from_status`.
- `transport.rs` — `trait DecisionTransport { async post(TransportRequest{body,timeout}) }`, `SystemOneClient<T>` (`ClientOptions{model, keep_alive=30m, timeout=400ms, context_tokens=2050}`), `block_on` mini executor, test mock.
- `questions.rs` / `state.rs` / `cascade.rs` / `text.rs` — 12 fine-grained intent classes summed into control/conversation/dictation; dictation lead-in rule ("dicta", "escribe:", "apunta", "toma nota", "pon en el cuadro…"); sans-IO `Cascade` (intent → agent+action → engine+project; `switch_project` class → project only); state JSON bounded by context/64 KiB.
- `policy.rs` — fail-closed `policy()` → Execute / Confirm{reason} / ExplicitOnly / SwitchProject / Dictate / Conversation{reason, top2}. p = min over needed questions; flagged approvals (require_explicit_approval / never_auto_approve) → ExplicitOnly; destructive needs p ≥ 0.95 + literal id; thresholds with floors.
- `project.rs` — STT-tolerant trigram `Scorer`, `prefilter` to 12, `select_project`.
- `command.rs` — `Command`, `Capabilities` (pause/resume refused until G4), `CommandExecutor` trait, `dispatch`.
- `log.rs` / `eval.rs` — `decisions.jsonl` with 10 MB rotation; `replay` / `compare_logs`, deterministic report.
- `src/bin/decide-eval` — headless eval CLI with a std::net plain-HTTP transport (kept out of the library).

## Why
Backlog M6 decision layer: "the model classifies, the code decides". Key finding: tev1:0.8b scores 66/100 with 3 abstract intent criteria; fine-grained classes + dictation rule give 96/100.

## Verification
`cargo fmt -p decide`, `cargo clippy -p decide --all-targets -- -D warnings`, `cargo test -p decide` (70 lib + 2 bin + 1 integration tests, proptests 4000 cases on policy), `cargo xtask check-deps` OK. Live (#[ignore], local Ollama 0.35 + tev1:0.8b): intent 96/100; log→replay round trip agrees 100 %.

## Remaining for the app
reqwest `DecisionTransport` (config from `/config/decision-model`, env key, redaction); concrete `CommandExecutor` over pando-client; append `DecisionRecord`s; `pando-assistant decisions eval` subcommand over `decide::eval`; confirm/explicit-only/pill UI with locale copy.
