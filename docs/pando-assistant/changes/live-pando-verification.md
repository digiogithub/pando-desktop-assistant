---
created_at: 2026-10-12T10:58:05.828484784Z
updated_at: 2026-10-12T10:58:05.828484784Z
---
# Live verification against Pando v1.3.0

What: recorded live fixtures (permission allow/deny, question, steering, cancel, delegation, resurrected run, conclusion_injected, edit diff) and drove the real app on xvfb against a live `pando serve`.
Files: crates/pando-client/tests/fixtures/*.sse, tests/{chat_fixtures,model}.rs, src/model.rs (ChatModel::reset_live deferred cut, pending_reset), crates/app/src/cells/mapping.rs (label_for_task, strip_task_preamble).
Why: closes the "not tested against a real Pando" items of PDA-T-0027, 0065, 0071, 0072 (and evidence for 0055/56/57/62). Bugs: resurrected-run replay dropped the finished run's rows; task cells labelled "You are the task_id".
Setup: scratch project with `.pando.toml` `[Permissions] AutoApproveTools=false`, `[Mesnada.Delegation] Enabled/InjectIntoLiveLoop/ResurrectIdleLoop=true`.
Findings: no steering_queued/conclusion_queued emitted; cancel does not end a run blocked on a permission.
Verified: cargo test -p pando-client -p app, clippy -D warnings, check-deps; screenshots of chip, sidebar, history, permission dialog (Enter/Space do not approve), diff, question dialog, slash palette, resurrected notice, cells, task cancel.
See [[pando-assistant/changes/pando-client-core.md]], [[pando-assistant/plans/backlog-execution_plan.md]].
