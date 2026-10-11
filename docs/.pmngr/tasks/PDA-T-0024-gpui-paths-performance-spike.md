---
id: PDA-T-0024
type: task
title: GPUI paths performance spike
status: backlog
parent: PDA-US-0007
milestone: PDA-M-0001
author: mcp
labels: [spike, organism]
estimate: 2
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:02:23Z
---

## Description
Synthetic scene equivalent to the organism (≈2–3 k points, ~100 `paint_path` calls, alpha halos, dashes) on real GPUs (Intel/AMD integrated, NVIDIA, Apple) and lavapipe. Measure lyon tessellation CPU, MSAA passes and fps; test the order "all paths, then text".

## Acceptance Criteria
- Decision: direct GPUI rendering vs tiny-skia raster as a runtime fallback.
