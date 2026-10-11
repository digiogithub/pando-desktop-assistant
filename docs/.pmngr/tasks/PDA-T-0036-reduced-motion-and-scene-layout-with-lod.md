---
id: PDA-T-0036
type: task
title: Reduced motion and Scene::layout with LOD
status: done
parent: PDA-US-0010
milestone: PDA-M-0002
assignees: [claude-sonnet]
author: mcp
labels: [organism, a11y]
estimate: 3
created: 2026-10-11T21:48:44Z
updated: 2026-10-11T22:43:44Z
started: 2026-10-11T22:41:26Z
closed: 2026-10-11T22:43:44Z
---

## Description
Reduced motion profile (breathing/noise/ripple/wobble → 0, zero drift, critically damped springs, static organelles and dashes, waves → fade without expansion, beat/blink → alpha differences). `Scene::layout(size, sf)`: scale W/640, minimum stroke 0.75 px, LOD (160/96, 48/32, 70/30), labels and badges hidden below 200 px.

## Acceptance Criteria
- Snapshot at 118 px is legible; springs monotonic under reduced motion.
