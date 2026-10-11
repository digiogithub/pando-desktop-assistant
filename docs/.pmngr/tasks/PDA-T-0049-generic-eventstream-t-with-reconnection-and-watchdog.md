---
id: PDA-T-0049
type: task
title: Generic EventStream<T> with reconnection and watchdog
status: backlog
parent: PDA-US-0013
milestone: PDA-M-0003
author: mcp
labels: [pando-client]
estimate: 3
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:03:06Z
---

## Description
On top of `SseParser`: `Content-Type` check, named and unnamed events, `Unknown{raw}`, exponential reconnection 1→30 s for notifications/projects/config, read-idle watchdog combined with polling (Pando sends no keep-alive), `Last-Event-ID` support when available (G9).

## Acceptance Criteria
- Tests: HTML with 200 ⇒ `Protocol`; connection drop ⇒ reconnects.
