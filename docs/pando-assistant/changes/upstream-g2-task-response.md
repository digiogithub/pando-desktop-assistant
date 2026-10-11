---
created_at: 2026-10-11T22:22:06.021781756Z
updated_at: 2026-10-11T22:22:06.021781756Z
---
# Upstream G2: TaskResponse with full fields (PDA-T-0156)

Builds on [[pando-assistant/research/upstream-gap-review.md]].

**What changed**: the orchestrator `TaskResponse` DTO now carries optional (omitempty) fields already present on `models.Task`: work_dir, tags, dependencies, error, exit_code, output_tail, started_at, completed_at, acp_session_id, acp_mode, acp_status, parent_session_id, parent_task_id, project_id, project_path, project_name, conclusion. Additive, backward compatible. `tokens` is still 0 because `models.Task` has no token counter.

**Files and symbols**: Pando repo `internal/api/handlers_orchestrator.go` (`TaskResponse`, `taskToResponse`), `internal/api/handlers_orchestrator_test.go` (`TestTaskToResponseFullFields`, `TestTaskToResponseMinimalPayloadUnchanged`), `docs/delegation.md`.

**Why**: G2/G3 of the gap review; the assistant needs the conclusion, error and project data over HTTP.

**Verification**: `go build ./...` and `go test ./internal/api` pass. Branch `task/PDA-T-0156-task-response-fields`, worktree `/www/MCP/Pando/pando-wt/pda-t-0156`, commit 0f00fd924 (base main 839540537). Not pushed; in review.
