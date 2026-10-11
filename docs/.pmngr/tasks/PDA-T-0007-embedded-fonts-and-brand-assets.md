---
id: PDA-T-0007
type: task
title: Embedded fonts and brand assets
status: done
parent: PDA-US-0002
milestone: PDA-M-0001
assignees: [claude-sonnet]
author: mcp
labels: [shell, design]
estimate: 2
created: 2026-10-11T21:47:40Z
updated: 2026-10-11T22:42:42Z
started: 2026-10-11T22:42:19Z
closed: 2026-10-11T22:42:42Z
---

## Description
Embed static TTFs of Space Grotesk (400–600), JetBrains Mono (300/400), Zen Kaku Gothic New 300 (OFL) with `include_bytes!` and register them before the first window; ship OFL licenses as resources; make the SVGs from `design-system/brand/` accessible via `img()`.

## Acceptance Criteria
- Linux test checks that the 3 families resolve.
- The two-tone wordmark is painted with `img()` (not `paint_svg`, which is monochrome).
