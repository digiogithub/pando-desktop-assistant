---
id: PDA-T-0073
type: task
title: Steering with "En cola" chips and Stop button
status: done
parent: PDA-US-0020
milestone: PDA-M-0004
assignees: [claude-sonnet]
author: mcp
labels: [chat]
estimate: 2
created: 2026-10-11T21:51:27Z
updated: 2026-10-12T08:20:17Z
started: 2026-10-11T22:46:56Z
closed: 2026-10-12T08:20:17Z
---

## Description
With an active run, sending calls `/steer` (409 `not_busy` ⇒ send as a normal prompt); chips stay queued until `steering_injected` (close the bubble, insert the user text, open a new one); Stop → `/cancel` + `/goal/cancel` if there is a goal.

## Acceptance Criteria
- Steer/done race handled without losing the text.
