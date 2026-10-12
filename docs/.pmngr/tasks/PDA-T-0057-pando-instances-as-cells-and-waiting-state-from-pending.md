---
id: PDA-T-0057
type: task
title: Pando instances as cells and Waiting state from pending items
status: done
parent: PDA-US-0014
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [organism, pando-client]
estimate: 2
created: 2026-10-11T21:50:04Z
updated: 2026-10-12T08:26:16Z
started: 2026-10-12T07:42:16Z
closed: 2026-10-12T08:26:16Z
---

## Description
Cells for non-primary instances (`/instances`); Waiting override when the primary session has pending items (`/sessions/{id}/pending`) and, with G2/G12, when the task has `acp_status.mode == permission`.

## Acceptance Criteria
- Pending permission on the primary session ⇒ cell/body shows `?` and inward vesicles.
