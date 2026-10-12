---
id: PDA-T-0043
type: task
title: Performance hardening and frame-time overlay
status: in_review
parent: PDA-US-0011
milestone: PDA-M-0002
assignees: [claude-sonnet]
author: mcp
labels: [organism, perf]
estimate: 3
created: 2026-10-11T21:48:44Z
updated: 2026-10-12T07:41:29Z
started: 2026-10-12T07:41:16Z
---

## Description
Discs grouped by alpha bucket, buffer reuse, frame-time overlay (`profiler` feature), measurement at 4K on integrated GPU; quads for discs only if the passes are the bottleneck.

## Acceptance Criteria
- 4K at 60 fps on integrated GPU; ≤ 4 ms CPU/frame with 6 cells at 640 px.
