---
id: PDA-US-0011
type: story
title: As a user I want to see the living organism in the window at 60 fps and interact with it
status: backlog
priority: high
parent: PDA-EP-0003
milestone: PDA-M-0002
author: mcp
labels: [organism, shell]
created: 2026-10-11T21:47:55Z
updated: 2026-10-11T22:05:25Z
---

## Description
`OrganismView` in GPUI: painting the draw-list, text cache, pointer/hover/click/keyboard/a11y, rAF with pause when hidden, demo with fake agents and state selector, visual verification against Chrome and raster fallback.

## Acceptance Criteria
- Implements PDA-SP-0004.R6, R7.
- SSIM ≥ 0.97 per state; 60 fps at 640 px (6 and 32 cells) on Linux X11/Wayland, macOS and Windows.

## Notes
Tasks T9–T11, T14 of `docs/research/organism-port.md`.
