---
id: PDA-T-0059
type: task
title: "Cell actions: cancel, delete, relaunch, approve/deny"
status: backlog
parent: PDA-US-0015
milestone: PDA-M-0003
author: mcp
labels: [shell, pando-client]
estimate: 2
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:02:19Z
---

## Description
Buttons depending on engine/status; cancel and delete with confirmation (`confirm()` returns false without Root); relaunch = `POST /orchestrator/tasks` with the same prompt/engine/model; approve/deny delegate to the approvals ledger (EP Human-in-the-loop).

## Acceptance Criteria
- A 409 on cancel of a terminal task shows a toast, not a generic error.
