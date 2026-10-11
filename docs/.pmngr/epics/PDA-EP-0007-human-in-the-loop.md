---
id: PDA-EP-0007
type: epic
title: Human-in-the-loop
status: backlog
priority: critical
milestone: PDA-M-0004
author: mcp
labels: [chat, safety]
created: 2026-10-11T21:45:36Z
updated: 2026-10-11T22:02:43Z
---

## Description
Agent permissions and questions: dialogs, queue, fail-closed behavior, auto-approve, global indicator and voice response.

## Acceptance Criteria
- PDA-SP-0003.R1–R6 verified; fail-closed tests use an injectable clock.

## Notes
Spec: PDA-SP-0003. Base: `PendingApprovals` from Bitácora (`agents/approvals.rs`).
