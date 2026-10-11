---
created_at: 2026-10-11T22:47:17.836115405Z
updated_at: 2026-10-11T22:47:17.836115405Z
tags:
    - change
    - pando-client
---
# pando-client chat model: ChatModel, approvals, steering, usage

Builds on [[pando-assistant/changes/pando-client-api-layer]] and [[pando-assistant/plans/backlog-execution_plan.md]]. Tasks: PDA-T-0063 (done), PDA-T-0082 (done), model parts of PDA-T-0065, 0073, 0077, 0078 (views pending in app).

## What changed
- `approvals.rs`: fail-closed `Approvals` ledger, `Decision`, `Origin` (Explicit|Voice|Auto), `DenyReason`, `Refused`, outbox + `deliver`, `Clock`/`ManualClock`.
- `model.rs`: `ChatModel` (transcript `Item`/`ItemKind`, `revision`, generations, `reset_live`, `load_history`, `apply`/`apply_follow`, `submit`, `decide`, goal banner, plan/todos, `render_debug`).
- `steering.rs`: `SteeringQueue`, `SendPlan`, `StopPlan`, `send_steer`, `execute_stop`.
- `usage.rs`: `UsageState`/`UsageSnapshot`. Finding: `token_usage` events are cumulative session totals; estimated ones are provisional.
- `chat.rs`: `Session` now carries cache/reasoning tokens; `FoldedItem` keeps unknown/binary parts.

## Verification
`cargo clippy -p pando-client --all-targets -- -D warnings`, `cargo test -p pando-client` (46 unit + integration, snapshots for every fixture), `cargo xtask check-deps`, `cargo deny check licenses`. Commits 62a6f4a (approvals) and 7152829 (model).
