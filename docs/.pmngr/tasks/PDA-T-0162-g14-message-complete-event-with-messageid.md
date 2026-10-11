---
id: PDA-T-0162
type: task
title: G14 · message_complete event with MessageID
status: backlog
priority: medium
parent: PDA-US-0047
milestone: PDA-M-0004
author: mcp
labels: [upstream-pando]
estimate: 2
created: 2026-10-11T21:57:00Z
updated: 2026-10-11T22:03:16Z
---

## Description
Emit `message_complete {messageId, text}` in `AgentEventTypeResponse` (currently dropped in `handlers_chat.go:711-712`) and expose `MessageID` in deltas.

## Acceptance Criteria
- The assistant knows what to say without accumulating heuristically.
