---
id: PDA-T-0082
type: task
title: Fail-closed approvals ledger
status: done
parent: PDA-US-0024
milestone: PDA-M-0004
assignees: [claude-sonnet]
author: mcp
labels: [safety]
estimate: 3
created: 2026-10-11T21:51:27Z
updated: 2026-10-11T22:42:55Z
started: 2026-10-11T22:42:46Z
closed: 2026-10-11T22:42:55Z
---

## Description
Port `PendingApprovals` (Bitácora `agents/approvals.rs`): only an explicit `Decision::Approve` approves; `DenyReason::{Timeout, PanelClosed, SessionSwitch, Quit, Cancelled, ServerExpired}`; injectable clock; the `require_explicit_approval`/`never_auto_approve` flags block shortcuts (voice, remember).

## Acceptance Criteria
- Tests for each denial reason and for the flags.
