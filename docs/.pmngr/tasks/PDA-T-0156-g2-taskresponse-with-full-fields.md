---
id: PDA-T-0156
type: task
title: G2 · TaskResponse with full fields
status: in_review
priority: high
parent: PDA-US-0046
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [upstream-pando]
estimate: 3
created: 2026-10-11T21:56:59Z
updated: 2026-10-11T22:22:06Z
started: 2026-10-11T22:21:58Z
---

## Description
Add additively (or `?full=1`): `work_dir, tags, error, exit_code, output_tail, acp_session_id, acp_mode, acp_status, parent_session_id, project_id/name/path, conclusion, dependencies, started_at, completed_at`; real `tokens`.

## Acceptance Criteria
- Existing clients do not break; serialization test.
