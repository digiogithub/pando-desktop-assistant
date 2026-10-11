---
id: PDA-T-0047
type: task
title: "ConnectionService: states, 30 s health, backoff and version gate"
status: done
parent: PDA-US-0012
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [pando-client]
estimate: 3
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:30:52Z
started: 2026-10-11T22:29:55Z
closed: 2026-10-11T22:30:52Z
---

## Description
Service on the tokio runtime with states `Connecting | Connected{version, project} | Unavailable{reason} | Unauthorized | TooOld{min}`; probe every 30 s when connected and 1→30 s backoff when disconnected; events to the UI; coordinated close/reopen of all streams.

## Acceptance Criteria
- Stopping Pando ⇒ `Unavailable` within 30 s; starting it ⇒ `Connected` and streams reopened.
