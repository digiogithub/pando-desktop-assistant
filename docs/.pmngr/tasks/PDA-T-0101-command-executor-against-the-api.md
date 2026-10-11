---
id: PDA-T-0101
type: task
title: Command executor against the API
status: in_progress
parent: PDA-US-0029
milestone: PDA-M-0006
assignees: [claude-opus]
author: mcp
labels: [decide]
estimate: 5
created: 2026-10-11T21:52:55Z
updated: 2026-10-11T22:42:36Z
started: 2026-10-11T22:42:36Z
---

## Description
cancel; status (speakable summary from `TaskResponse`: status, progress, current tool); open (select cell); spawn (`POST /orchestrator/tasks` with engine/project); retry/relaunch; approve/deny (via the approvals ledger); "háblale" / "vuelve" (target); pause/resume ⇒ "no disponible" until G4.

## Acceptance Criteria
- Commands run < 50 ms after the decision (excluding network); they never go through the LLM.
