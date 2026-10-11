---
id: PDA-T-0016
type: task
title: Correct planning docs with verified findings
status: done
parent: PDA-US-0005
milestone: PDA-M-0001
assignees: [claude-opus]
author: mcp
labels: [docs]
estimate: 2
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:20:55Z
started: 2026-10-11T22:13:55Z
closed: 2026-10-11T22:20:55Z
---

## Description
Update `docs/01`–`06` and `design-system/README.md` §6: ephemeral `X-Pando-Token` header via `/api/v1/token`; there is no `POST /sessions`; real SSE events (`todos_update`, no `response`, `done`); notifications do not carry the task lifecycle; `TaskResponse` projected; conclusion `success|partial|failed|blocked`; real needle API (bytes, libc++, no depth, streaming); rubato 5; global-hotkey X11 only; `Entity<Organism>` and `window.request_animation_frame()`.

## Acceptance Criteria
- No **[verify]** left unresolved or without a linked task.
