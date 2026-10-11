---
id: PDA-M-0003
type: milestone
title: M2 · Connection to Pando and live cells
status: backlog
author: mcp
created: 2026-10-11T21:40:29Z
updated: 2026-10-11T22:01:59Z
due: 2026-11-27
---

## Description
`pando-client` discovers the instance, obtains a token, trusts the local CA and maintains connection state. mesnada tasks (`/api/v1/orchestrator/tasks`) and instances become real cells with mitosis and death.

## Acceptance Criteria
- Automatic connection to `pando serve`/`app` (https) and `pando desktop` (http) on loopback without configuration.
- Spawn/cancel of a task in Pando ⇒ the cell is born/dies in < 1.5 s (polling) or < 200 ms (with G1 stream).
- The side panel of a cell shows label, engine, status, project and output.

## Notes
Refs: `docs/research/pando-api.md` §1, §2.4, §6.2 (G1, G2).
