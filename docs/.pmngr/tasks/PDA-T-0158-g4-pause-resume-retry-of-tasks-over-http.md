---
id: PDA-T-0158
type: task
title: G4 · Pause/resume/retry of tasks over HTTP
status: backlog
priority: medium
parent: PDA-US-0046
milestone: PDA-M-0006
author: mcp
labels: [upstream-pando]
estimate: 3
created: 2026-10-11T21:57:00Z
updated: 2026-10-11T22:03:16Z
---

## Description
`POST /orchestrator/tasks/{id}/pause`, `POST …/resume {prompt, model?}` (returns the new task), expose `Retry`/`Relaunch` (`orchestrator.go:1525,1628`).

## Acceptance Criteria
- Voice actions "pausa"/"reanuda" enabled in the assistant.
