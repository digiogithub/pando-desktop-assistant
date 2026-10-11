---
id: PDA-T-0031
type: task
title: JS golden harness (harness.mjs)
status: done
parent: PDA-US-0009
milestone: PDA-M-0002
assignees: [claude-sonnet]
author: mcp
labels: [organism, testing]
estimate: 5
created: 2026-10-11T21:48:44Z
updated: 2026-10-11T22:43:44Z
started: 2026-10-11T22:29:46Z
closed: 2026-10-11T22:43:44Z
---

## Description
`design-system/reference/harness.mjs` (Node, no deps): loads the `<script>` of `organism.html`, stubs for document/canvas/fetch/rAF, `Math.random` → mulberry32(seed), scripted timeline (state changes, spawn, kill, pointer path), fixed dt 1/60, logs ctx calls (points, arcs, styles, width, dash, alpha, text) as JSON per frame {0,1,30,120,600}.

## Acceptance Criteria
- Two runs produce identical goldens; covers 5 states, 4 engines, 5 statuses.
