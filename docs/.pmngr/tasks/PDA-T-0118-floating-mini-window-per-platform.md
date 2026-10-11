---
id: PDA-T-0118
type: task
title: Floating mini window per platform
status: backlog
parent: PDA-US-0035
milestone: PDA-M-0008
author: mcp
labels: [shell, organism]
estimate: 5
created: 2026-10-11T21:54:29Z
updated: 2026-10-11T22:02:08Z
---

## Description
380×440 window without titlebar, transparent background with Bosque quad radius 32, orb at 118 px (same simulation), state + transcript + hotkey hint + badge; `PopUp` on macOS/Windows, X11 PopUp with custom dragging, `LayerShell` on wlroots/KDE, normal window on GNOME; `inactive_frame_interval` 33–50 ms; remembered position; click → main window.

## Acceptance Criteria
- ≤ 2 % CPU idle; behavior documented per platform.
