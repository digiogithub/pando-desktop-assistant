---
id: PDA-T-0072
type: task
title: "run_seq protocol: reattach, replay and resurrected runs"
status: in_review
parent: PDA-US-0020
milestone: PDA-M-0004
assignees: [claude-sonnet]
author: mcp
labels: [chat, pando-client]
estimate: 5
created: 2026-10-11T21:51:27Z
updated: 2026-10-11T22:39:59Z
started: 2026-10-11T22:39:05Z
---

## Description
`seenRunSeq` per session, polling `/pending` every ~4 s for the followed sessions (primary + open), attach to `/sessions/{id}/stream` when `run_seq` increases, bubble reset before replay, history reload if the replay is empty, reattach if the stream closes without `done`; use the global stream G5 when it exists.

## Acceptance Criteria
- Scenarios of PDA-SP-0002.R4 covered with a mock and with real Pando.
