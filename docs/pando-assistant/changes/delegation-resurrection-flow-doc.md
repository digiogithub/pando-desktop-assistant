---
created_at: 2026-10-12T08:28:20.899411299Z
updated_at: 2026-10-12T08:28:20.899411299Z
tags:
    - pando-assistant
    - change
    - docs
    - pando-api
    - delegation
---
# Delegation → conclusion → resurrection flow documented (PDA-T-0028)

**What changed:** new subsection "Delegation → conclusion → resurrection flow (PDA-T-0028)" in `docs/04-pando-api.md`: mermaid sequence diagram (spawn tool, task polling, Case A `conclusion_queued` → `conclusion_injected` with an active parent, Case B resurrected background run with no client discovered via `run_seq` + `/pending` polling and reattach, `[delegated-result …]` block in history), a step → client-code table, and notes (run ownership, stale >1h conclusions not resurrecting, what to speak, upstream G2/G3/G5 status, fixture review).

**Files & symbols:** `docs/04-pando-api.md` only. Points to `pando-client`: `TaskTracker::update`/`TaskChange` (api.rs), `follow_session`/`seen_for`/`should_attach`/`FollowEvent` (follow.rs), `parse_conclusion`/`parse_delegated_results` (conclusion.rs), `ChatModel::apply`/`load_history`, `NoticeKind::{ConclusionQueued,ConclusionInjected,Resurrected}` (model.rs).

**Why:** PDA-T-0028 (story PDA-US-0008).

**Ground truth / verification:** Pando `docs/delegation.md`, `internal/api/handlers_chat.go:692-710` (payload `{session_id,message}`), `agent.go:708/757`, `delegation_supervisor.go`, `api/resume_run.go`. Fixtures: `session_reattach.sse` (real) confirms replay shape; conclusion/resurrected only synthetic (`synthetic_events.sse`, `tests/follow.rs`); live capture pending PDA-T-0027. Upstream G2 branch `task/PDA-T-0156-task-response-fields` (commit 0f00fd924) pending owner review. Commit d4f792f. Docs-only, no build needed.

Links: [[pando-assistant/plans/backlog-execution_plan.md]], [[docs/research/pando-api.md]], [[docs/research/upstream-gap-review.md]]
