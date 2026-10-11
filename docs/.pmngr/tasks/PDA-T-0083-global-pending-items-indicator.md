---
id: PDA-T-0083
type: task
title: Global pending-items indicator
status: backlog
parent: PDA-US-0024
milestone: PDA-M-0004
author: mcp
labels: [safety, shell]
estimate: 2
created: 2026-10-11T21:51:27Z
updated: 2026-10-11T22:02:51Z
---

## Description
Counter of agents waiting (header; tray and mini window when they exist), aggregating `/pending` from followed sessions (G12 when it exists); clicking goes to the first pending item.

## Acceptance Criteria
- A pending item in an unselected session increments the counter.
