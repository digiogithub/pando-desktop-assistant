---
id: PDA-T-0055
type: task
title: "CellSource: task polling, diff and Delegating"
status: backlog
parent: PDA-US-0014
milestone: PDA-M-0003
author: mcp
labels: [organism, pando-client]
estimate: 3
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:03:06Z
---

## Description
Service that polls `/orchestrator/tasks` (1 s visible, 5 s hidden), computes additions/removals/changes by id, marks `is_new` for mitosis and emits the Delegating overlay when a created task is detected; designed to be replaced by the G1 stream.

## Acceptance Criteria
- Mock test: a sequence of responses produces the expected cell events.
