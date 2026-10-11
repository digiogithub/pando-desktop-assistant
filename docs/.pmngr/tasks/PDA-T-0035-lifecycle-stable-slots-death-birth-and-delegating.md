---
id: PDA-T-0035
type: task
title: "Lifecycle: stable slots, death, birth and Delegating"
status: done
parent: PDA-US-0010
milestone: PDA-M-0002
assignees: [claude-sonnet]
author: mcp
labels: [organism]
estimate: 5
created: 2026-10-11T21:48:44Z
updated: 2026-10-11T22:43:44Z
started: 2026-10-11T22:38:20Z
closed: 2026-10-11T22:43:44Z
---

## Description
Slots assigned on appearance and released on death (angle over `max(n, slots_in_use)`, compaction after N s per the design decision); `Life::Dying{since}` 0.6 s (alpha 1→0, pseudopod 1→0, bulge fades) and revives if it reappears; birth emits its own wave; `BodyMachine` with a 1.5 s Delegating overlay; `dim` 60 % for pending. Outside parity mode.

## Acceptance Criteria
- Tests: no slot jump when adding/removing; dying cell revives; transition table in `docs/06` covered.
