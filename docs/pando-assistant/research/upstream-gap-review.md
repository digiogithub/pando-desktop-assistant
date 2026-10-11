---
created_at: 2026-10-11T22:20:21.413818711Z
updated_at: 2026-10-11T22:20:21.413818711Z
tags:
    - research
    - pando-api
    - upstream-pando
    - pando-assistant
---
# Upstream gap review: what Pando already covers (G1–G17)

Repo copy: `docs/research/upstream-gap-review.md` (pando-assistant). Date: 2026-10-12.
Scope: gaps G1–G17 in `docs/research/pando-api.md` §6.2, tasks PDA-T-0155..0170 (epic PDA-EP-0015).
Constraint from the product owner: the existing AG-UI API and WebUI HTTP+SSE API should be enough. Anything added to Pando must be generic and indispensable.
Related: [[pando-assistant/plans/backlog-execution_plan.md]], ADR 0002 (REST + chat/stream, not AG-UI), [[pando-api]].

## Summary

| Gap | Task | Class | One line |
|---|---|---|---|
| G1 task push events | PDA-T-0155 | NOT-NEEDED | Poll GET /api/v1/orchestrator/tasks every 1–2 s, as the WebUI does (OrchestratorView.tsx, ChatInfoSidebar.tsx). |
| G2 full task fields | PDA-T-0156 | REQUIRED (tiny) | taskToResponse drops fields models.Task already serialises (work_dir, tags, error, exit_code, output_tail, started/completed_at, dependencies, acp_*, parent_session_id, project_*, conclusion). Add them as additive omitempty fields. |
| G3 structured conclusion | PDA-T-0157 | COVERED by G2 | Conclusion reaches HTTP via G2. The event half is not needed (polling sees the terminal status). |
| G4 pause/resume/retry | PDA-T-0158 | PARTIAL | Exists on the embedded mesnada server (internal/mesnada/server/gin.go POST /api/tasks/:id/pause\|resume\|retry; port 9767, no token, not in the registry). Mirror into /api/v1/orchestrator/tasks/{id}/... (optional). |
| G5 global activity | PDA-T-0160 | PARTIAL | GET /api/v1/instances/{id}/stream proxies the IPC bridge (session.update/deleted, llm.token, tool.start/end, llm.end for all CoderAgent sessions), but only on the IPC primary, with no permission or question topics. Fallback: poll /sessions is_running plus /pending run_seq. Recommended generic GET /api/v1/events. |
| G6 decision proxy | PDA-T-0165 | NOT-NEEDED | The assistant owns its decide provider config. The playground endpoint is router-only. |
| G7 attachments | PDA-T-0166 | NOT-NEEDED (later) | agent.Run already takes attachments; ChatRequest lacks them. AG-UI accepts parts but drops non-text ones. |
| G8 POST /sessions | PDA-T-0163 | NOT-NEEDED | Lazy creation via chat/stream (sessionId "" plus model), PATCH the title, child proxy for a project. |
| G9 SSE ids | PDA-T-0161 | NOT-NEEDED | run/runSeq plus replay reattach. |
| G10 capabilities | PDA-T-0163 | COVERED | /health version, GET /api/v1/version, plus probe and 404 handling. |
| G11 v2 DTO | PDA-T-0164 | NOT-NEEDED | Client-side mapper. |
| G12 cross-session pending | PDA-T-0159 | PARTIAL, fold into G5 | Poll /sessions/{id}/pending for running sessions; natively, pending_changed on the G5 stream. |
| G13 registry | PDA-T-0168 | NOT-NEEDED | Same hard-coded path on both sides; probe https/http. |
| G14 message_complete | PDA-T-0162 | NOT-NEEDED | Accumulate content_delta until done. |
| G15 token mutex | PDA-T-0169 | NOT-NEEDED (for us) | The assistant never rotates the token; report the race as a Pando bug. |
| G16 events/memory REST | PDA-T-0167 | NOT-NEEDED | kb/search REST exists; ask the Pando agent in chat for event and recall queries. |
| G17 WebUI bugs | PDA-T-0170 | NOT-NEEDED (for us) | Still present (CreateTaskDialog sends description; desktop notifier sends no token); report as Pando bugs. |

## Why not AG-UI as the primary transport

internal/agui (invariants I2/I3/I7 in doc.go) runs its own agent pool and its own permission and userinput services. It never sees WebUI or TUI sessions, their pending approvals or runs resurrected there. It has no orchestrator view, is off by default and uses its own token. It does provide typed events, TEXT_MESSAGE_END with messageId, typed thread messages, reattach, cancel, fail-closed HITL and per-thread StateDoc.SubAgents. ADR 0002 stands. An isolated [AGUI.Profiles] assistant profile stays possible with no Pando change.

## Recommended upstream set

1. PDA-T-0156 (G2): required; absorbs G3's task half. Additive TaskResponse fields plus a unit test on taskToResponse.
2. PDA-T-0160 (G5, widened to absorb G12): recommended, not blocking. One GET /api/v1/events SSE multiplexing existing pubsub: Sessions.Subscribe, bgRunner/CoderAgent run_started/run_finished {sessionId, runSeq}, Permissions/UserInput.Subscribe → pending_changed. The MVP ships with polling.
3. PDA-T-0158 (G4): optional mirror of the existing mesnada REST handlers; otherwise hide pause/resume.

Cancel or park all the others (0155, 0157, 0159 merged into 0160, 0161, 0162, 0163, 0164, 0165, 0166, 0167, 0168). 0169 and 0170 are Pando bugs to report on their own.

See the repo file for the full per-gap analysis with file:symbol citations.
