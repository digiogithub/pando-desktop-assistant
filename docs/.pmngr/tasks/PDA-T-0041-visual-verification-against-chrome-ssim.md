---
id: PDA-T-0041
type: task
title: Visual verification against Chrome (SSIM)
status: in_review
parent: PDA-US-0011
milestone: PDA-M-0002
assignees: [claude-sonnet]
author: mcp
labels: [organism, testing]
estimate: 3
created: 2026-10-11T21:48:44Z
updated: 2026-10-12T07:41:29Z
started: 2026-10-12T07:41:16Z
---

## Description
Capture fixed frames (seed + frame) from the harness in headless Chrome and from the app (raster or GPUI capture); per-state SSIM comparison; report with diffs for design review.

## Acceptance Criteria
- SSIM ≥ 0.97 per state; design sign-off recorded.
