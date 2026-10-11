---
id: PDA-T-0160
type: task
title: G5 · SSE /api/v1/sessions/events (global activity)
status: backlog
priority: low
parent: PDA-US-0047
milestone: PDA-M-0004
author: mcp
labels: [upstream-pando]
estimate: 5
created: 2026-10-11T21:57:00Z
updated: 2026-10-11T22:21:30Z
---

## Description
`run_started {sessionId, runSeq, kind: user|resurrected|steer}`, `run_finished`, `pending_changed`, `session_created|updated|deleted`.

## Acceptance Criteria
- Resurrected run detected by the client without polling.
