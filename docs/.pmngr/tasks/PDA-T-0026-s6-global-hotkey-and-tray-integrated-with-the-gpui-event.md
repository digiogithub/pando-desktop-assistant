---
id: PDA-T-0026
type: task
title: S6 · Global hotkey and tray integrated with the GPUI event loop
status: backlog
parent: PDA-US-0007
milestone: PDA-M-0001
author: mcp
labels: [spike, shell]
estimate: 3
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:02:37Z
---

## Description
`global-hotkey` (macOS main thread, Windows message loop, X11) with Pressed/Released; GlobalShortcuts portal via `ashpd` (Activated/Deactivated) on GNOME 48+ and KDE 6; `ksni` on Linux and `tray-icon` on macOS/Windows without blocking GPUI.

## Acceptance Criteria
- Hold-to-talk works on all; CLI fallback documented for Sway.
