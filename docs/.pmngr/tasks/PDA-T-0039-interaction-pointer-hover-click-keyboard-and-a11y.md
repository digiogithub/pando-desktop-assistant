---
id: PDA-T-0039
type: task
title: "Interaction: pointer, hover, click, keyboard and a11y"
status: in_review
parent: PDA-US-0011
milestone: PDA-M-0002
assignees: [claude-sonnet]
author: mcp
labels: [organism, a11y]
estimate: 3
created: 2026-10-11T21:48:44Z
updated: 2026-10-12T07:32:40Z
started: 2026-10-12T07:32:25Z
---

## Description
`on_mouse_move` → normalized pointer; hit-test 40 px × scale for click, hover 34 px; cursor per hitbox; keyboard focus on the canvas (Tab/arrows cycle through cells, Enter opens card, Esc deselects); accessible roles and labels from `semantics()`; `CellSelected(id)` event.

## Acceptance Criteria
- Selection possible by keyboard alone; screen reader announces "<label>, <engine>, <estado>".
