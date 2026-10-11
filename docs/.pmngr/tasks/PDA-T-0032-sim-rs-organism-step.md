---
id: PDA-T-0032
type: task
title: "sim.rs: Organism::step"
status: backlog
parent: PDA-US-0009
milestone: PDA-M-0002
author: mcp
labels: [organism]
estimate: 5
created: 2026-10-11T21:48:44Z
updated: 2026-10-11T22:02:37Z
---

## Description
Order: amplitude → state machine/wave emission → wave advance/cleanup → cell reconciliation → spring/flow/hover per cell → organelles → nucleus tracking (moved from draw, fps-independent lerp `1−(1−0.08)^(dt·60)`). dt clamp 0.05.

## Acceptance Criteria
- Parity mode state matches goldens (1e-6) at frames 1/30/120/600.
