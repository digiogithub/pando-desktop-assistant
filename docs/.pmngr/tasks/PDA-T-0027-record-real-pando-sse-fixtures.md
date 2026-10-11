---
id: PDA-T-0027
type: task
title: Record real Pando SSE fixtures
status: in_review
parent: PDA-US-0008
milestone: PDA-M-0001
assignees: [claude-sonnet]
author: mcp
labels: [spike, pando-client]
estimate: 2
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:23:12Z
started: 2026-10-11T22:21:15Z
---

## Description
Script that starts `pando serve` with a test project and records raw bytes for: simple chat, chat with tools (bash, edit, read), permission, question, steering, cancel, goal, slash command, delegation + `conclusion_*`, reattach with replay, `/sessions/{id}` and `/orchestrator/tasks`.

## Acceptance Criteria
- Fixtures versioned with the Pando commit; script re-runnable.
