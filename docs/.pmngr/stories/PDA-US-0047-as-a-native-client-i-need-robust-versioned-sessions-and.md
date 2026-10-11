---
id: PDA-US-0047
type: story
title: As a native client I need robust, versioned sessions and streaming
status: backlog
priority: high
parent: PDA-EP-0015
milestone: PDA-M-0004
author: mcp
labels: [upstream-pando]
created: 2026-10-11T21:56:18Z
updated: 2026-10-11T22:03:36Z
---

## Description
Gaps G5 (global session activity stream), G8 (explicit session creation), G9 (SSE ids + Last-Event-ID), G10 (capabilities), G11 (typed v2 DTO), G14 (final message event).

## Acceptance Criteria
- Client can reattach without resetting the bubble and without `run_seq` polling.
