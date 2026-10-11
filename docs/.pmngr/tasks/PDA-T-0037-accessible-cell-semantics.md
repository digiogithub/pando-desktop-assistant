---
id: PDA-T-0037
type: task
title: Accessible cell semantics
status: in_progress
parent: PDA-US-0010
milestone: PDA-M-0002
assignees: [claude-sonnet]
author: mcp
labels: [organism, a11y]
estimate: 2
created: 2026-10-11T21:48:44Z
updated: 2026-10-11T22:42:04Z
started: 2026-10-11T22:42:04Z
---

## Description
`Organism::semantics()` → list of cells with bounds, localizable label, engine and status, for hitboxes, tooltips and the accessibility tree.

## Acceptance Criteria
- Test: semantics match drawn positions (±1 px).
