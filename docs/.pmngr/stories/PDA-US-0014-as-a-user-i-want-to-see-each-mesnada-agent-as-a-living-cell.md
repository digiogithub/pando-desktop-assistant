---
id: PDA-US-0014
type: story
title: As a user I want to see each mesnada agent as a living cell
status: backlog
priority: high
parent: PDA-EP-0005
milestone: PDA-M-0003
author: mcp
labels: [organism, pando-client]
created: 2026-10-11T21:49:06Z
updated: 2026-10-11T22:05:25Z
---

## Description
Cell source from `/orchestrator/tasks` (polling 1 s visible / 5 s hidden, diff by id; G1 stream when available) and `/instances`; engine/status mapping; Delegating overlay on creation; Waiting for pending items.

## Acceptance Criteria
- Implements PDA-SP-0005.R1–R3.
- Real spawn/cancel reflected in < 1.5 s.
