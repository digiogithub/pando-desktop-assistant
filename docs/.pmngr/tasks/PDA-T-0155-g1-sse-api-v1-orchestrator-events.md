---
id: PDA-T-0155
type: task
title: G1 · SSE /api/v1/orchestrator/events
status: backlog
priority: high
parent: PDA-US-0046
milestone: PDA-M-0003
author: mcp
labels: [upstream-pando]
estimate: 5
created: 2026-10-11T21:56:59Z
updated: 2026-10-11T22:03:01Z
---

## Description
Stream over the orchestrator event log (`UnseenEvents`, `RecentEvents`, `SubscribeCompletions`, `orchestrator.go:322-349,607`): `task_created | status_changed | progress | tool_call | completed` with the full task.

## Acceptance Criteria
- Cell appears < 200 ms after spawn; test in Pando.
