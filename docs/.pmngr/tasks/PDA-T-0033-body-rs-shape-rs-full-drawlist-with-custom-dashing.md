---
id: PDA-T-0033
type: task
title: "body.rs + shape.rs: full DrawList with custom dashing"
status: backlog
parent: PDA-US-0009
milestone: PDA-M-0002
author: mcp
labels: [organism]
estimate: 8
created: 2026-10-11T21:48:44Z
updated: 2026-10-11T22:02:37Z
---

## Description
`body_radius(angle)` with cached bulge terms; `shape(&Scene) -> DrawList` (`Poly`, `Curve` midpoint spline, `Circle`, `Discs` per alpha bucket, `Text`) in the order of §1.17; `dash_polyline` with phase (GPUI has no dash offset); analytic clipping of organelles; buffers reused with no per-frame allocations.

## Acceptance Criteria
- Golden parity: points ±1e-3 px, alpha ±1e-4; number of primitives stable across frames.
