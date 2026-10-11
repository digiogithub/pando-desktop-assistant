---
id: PDA-T-0009
type: task
title: Frameless main window with custom title bar
status: backlog
parent: PDA-US-0002
milestone: PDA-M-0001
author: mcp
labels: [shell]
estimate: 3
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:02:09Z
---

## Description
`WindowOptions` with transparent titlebar, `window_decorations: Client`, `app_owns_titlebar_drag`, `app_id` `dev.pando.assistant`, minimum size; 52 px title bar with wordmark, project pill and status chips (placeholders); kit border/resize on Linux. Branch according to the decorations granted.

## Acceptance Criteria
- Moving/resizing works on GNOME Wayland, KDE, X11, macOS and Windows.
