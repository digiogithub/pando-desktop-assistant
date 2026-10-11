---
id: PDA-T-0038
type: task
title: "OrganismView: GPUI painting of the DrawList"
status: backlog
parent: PDA-US-0011
milestone: PDA-M-0002
author: mcp
labels: [organism, shell]
estimate: 8
created: 2026-10-11T21:48:44Z
updated: 2026-10-11T22:02:51Z
---

## Description
Entity holding `Organism` and `last_instant`; `canvas(prepaint, paint)` that translates primitives to `PathBuilder::fill/stroke` + `window.paint_path` (all geometry as paths in order, text last with `shape_line` cached by label/size/alpha), circles as polygons in the batch, `StrokeOptions` with round caps. `request_animation_frame` only when visible; `observe_window_visibility` resets dt; `inactive_frame_interval` configurable.

## Acceptance Criteria
- 0 frames while the window is hidden; ≤ 3 path passes per frame.
