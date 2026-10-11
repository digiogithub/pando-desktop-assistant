---
id: PDA-T-0161
type: task
title: G9 · SSE event ids and Last-Event-ID
status: cancelled
priority: medium
parent: PDA-US-0047
milestone: PDA-M-0004
author: mcp
labels: [upstream-pando]
estimate: 3
created: 2026-10-11T21:57:00Z
updated: 2026-10-11T22:21:30Z
closed: 2026-10-11T22:21:30Z
---

## Description
`id: <runSeq>.<eventIndex>` in the chat stream and support for `Last-Event-ID` against the 500-event ring.

## Acceptance Criteria
- Reattach without duplicating deltas.
