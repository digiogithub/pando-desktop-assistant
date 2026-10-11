---
id: PDA-T-0047
type: task
title: "ConnectionService: states, 30 s health, backoff and version gate"
status: backlog
parent: PDA-US-0012
milestone: PDA-M-0003
author: mcp
labels: [pando-client]
estimate: 3
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:03:06Z
---

## Description
Service on the tokio runtime with states `Connecting | Connected{version, project} | Unavailable{reason} | Unauthorized | TooOld{min}`; probe every 30 s when connected and 1→30 s backoff when disconnected; events to the UI; coordinated close/reopen of all streams.

## Acceptance Criteria
- Stopping Pando ⇒ `Unavailable` within 30 s; starting it ⇒ `Connected` and streams reopened.
