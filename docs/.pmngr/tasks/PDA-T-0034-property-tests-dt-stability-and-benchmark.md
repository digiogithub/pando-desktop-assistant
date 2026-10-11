---
id: PDA-T-0034
type: task
title: Property tests, dt stability and benchmark
status: done
parent: PDA-US-0009
milestone: PDA-M-0002
assignees: [claude-sonnet]
author: mcp
labels: [organism, testing]
estimate: 3
created: 2026-10-11T21:48:44Z
updated: 2026-10-11T22:43:44Z
started: 2026-10-11T22:35:43Z
closed: 2026-10-11T22:43:44Z
---

## Description
proptest (10k cases): no NaN/inf, body polyline closed and radius > 0, cells inside the habitat, vesicles in [0,1), waves pruned, step(dt)×2 ≈ step(2dt), no energy blow-up with dt 0.05 × 10⁴, zero-length pseudopod with no NaN. criterion for 6 and 32 cells.

## Acceptance Criteria
- Bench < 0.3 ms (6) / < 1 ms (32) on a reference x86 machine.
