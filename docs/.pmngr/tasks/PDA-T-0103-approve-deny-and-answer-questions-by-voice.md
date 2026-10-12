---
id: PDA-T-0103
type: task
title: Approve, deny and answer questions by voice
status: in_review
parent: PDA-US-0029
milestone: PDA-M-0006
assignees: [claude-sonnet]
author: mcp
labels: [decide, safety]
estimate: 2
created: 2026-10-11T21:52:55Z
updated: 2026-10-12T08:47:05Z
started: 2026-10-12T08:47:05Z
---

## Description
Read aloud the target's pending item; "aprueba" / "deniega" / "la segunda opción" / "otro: …" mapped to `permissions/respond` and `questions/respond`; respects explicit-approval flags (requires click/keyboard).

## Acceptance Criteria
- Scenarios of PDA-SP-0003.R5 and R3 (never_auto_approve).
