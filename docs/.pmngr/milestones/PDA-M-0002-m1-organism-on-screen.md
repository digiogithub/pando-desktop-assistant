---
id: PDA-M-0002
type: milestone
title: M1 · Organism on screen
status: backlog
author: mcp
created: 2026-10-11T21:40:29Z
updated: 2026-10-11T22:01:59Z
due: 2026-11-13
---

## Description
Port of `design-system/reference/organism.html` to `crates/organism` (pure simulation) + GPUI adapter. Fake agents and keyboard state selector.

## Acceptance Criteria
- Draw-list parity with the JS harness goldens (pts ±1e-3 px, alpha ±1e-4).
- SSIM ≥ 0.97 against a Chrome capture per state.
- 60 fps at 640 px with 6 and 32 cells; ≤ 4 ms CPU/frame (6 cells); 0 frames with the window hidden.
- Hover, click and selection work.

## Notes
Refs: `docs/research/organism-port.md` §4 (critical path T1→T5→T9→T10).
