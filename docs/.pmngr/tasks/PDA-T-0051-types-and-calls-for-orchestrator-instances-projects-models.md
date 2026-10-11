---
id: PDA-T-0051
type: task
title: Types and calls for orchestrator, instances, projects, models and commands
status: done
parent: PDA-US-0013
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [pando-client]
estimate: 3
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:37:05Z
started: 2026-10-11T22:36:57Z
closed: 2026-10-11T22:37:05Z
---

## Description
`TaskResponse` (+ optional additive fields from G2), create/cancel/delete/get tasks, delegation metrics; `/instances`; `/projects` + `/projects/events` + open/close web + proxy router `/projects/{id}/web/api/v1/...`; `/models` + `PUT /models/active`; `/commands`; `/version`, `/project`; `/files/search`.

## Acceptance Criteria
- Proxy router test: a call to a child project rewrites the path and keeps headers.
