---
id: PDA-T-0157
type: task
title: G3 · Structured conclusion in events and in the task
status: cancelled
priority: high
parent: PDA-US-0046
milestone: PDA-M-0003
author: mcp
labels: [upstream-pando]
estimate: 3
created: 2026-10-11T21:56:59Z
updated: 2026-10-11T22:21:30Z
closed: 2026-10-11T22:21:30Z
---

## Description
Include `task_id` and `conclusion` (status, summary, follow_up, artifacts, confidence) in `conclusion_queued`, `conclusion_injected` and `resurrected`; `GET /orchestrator/tasks/{id}` → `conclusion`.

## Acceptance Criteria
- The assistant stops parsing `[delegated-result]` from history.
