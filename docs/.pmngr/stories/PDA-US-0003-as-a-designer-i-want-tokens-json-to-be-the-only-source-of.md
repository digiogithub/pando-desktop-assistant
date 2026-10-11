---
id: PDA-US-0003
type: story
title: As a designer I want tokens.json to be the only source of truth for visual design
status: backlog
priority: high
parent: PDA-EP-0001
milestone: PDA-M-0001
author: mcp
labels: [infra, design]
created: 2026-10-11T21:46:17Z
updated: 2026-10-11T22:02:01Z
---

## Description
`cargo xtask tokens` generates from `design-system/tokens.json`: Rust palette (`// GENERATED`), typographic scale, GPUI Kit theme (JSON), and motion/organism/engines/status constants. `--check` (CI) and `--contrast` (WCAG 4.5:1) modes.

## Acceptance Criteria
- Changing a color in tokens.json and regenerating changes the app without hand-editing Rust.
- CI fails if the generated files are outdated.

## Notes
Port `xtask/src/tokens.rs` from Bitácora and add generators for `motion`, `organism`, `engines`, `status`. Implements PDA-SP-0004.R2 (part).
