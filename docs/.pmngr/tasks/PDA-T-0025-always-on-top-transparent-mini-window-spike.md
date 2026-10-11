---
id: PDA-T-0025
type: task
title: Always-on-top transparent mini window spike
status: in_review
parent: PDA-US-0007
milestone: PDA-M-0001
assignees: [claude-sonnet]
author: mcp
labels: [spike, shell]
estimate: 2
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T23:03:56Z
started: 2026-10-11T23:03:53Z
---

## Description
Test `WindowKind::PopUp`/`Floating`/`LayerShell` + `WindowBackgroundAppearance::Transparent` on macOS, Windows, X11, KDE Wayland, GNOME Wayland and Sway. Custom dragging and keyboard focus.

## Acceptance Criteria
- Behavior matrix and choice per platform.
