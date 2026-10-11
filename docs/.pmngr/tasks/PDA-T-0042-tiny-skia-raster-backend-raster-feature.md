---
id: PDA-T-0042
type: task
title: tiny-skia raster backend (raster feature)
status: backlog
parent: PDA-US-0011
milestone: PDA-M-0002
author: mcp
labels: [organism]
estimate: 5
created: 2026-10-11T21:48:44Z
updated: 2026-10-11T22:02:51Z
---

## Description
Render the DrawList into a reused `Pixmap`, convert to BGRA (verify premultiplication with a test pattern), `RenderImage` + `paint_image`, `drop_image` of the previous frame; used for PNG goldens, exporting icons/GIFs, and as a fallback if the performance spike requires it.

## Acceptance Criteria
- SSIM ≥ 0.95 vs GPUI capture; flat memory over 10 min.
