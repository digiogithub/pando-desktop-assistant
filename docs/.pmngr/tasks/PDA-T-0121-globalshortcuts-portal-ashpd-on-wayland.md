---
id: PDA-T-0121
type: task
title: GlobalShortcuts portal (ashpd) on Wayland
status: in_review
parent: PDA-US-0036
milestone: PDA-M-0008
assignees: [claude-sonnet]
author: mcp
labels: [shell]
estimate: 3
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T08:08:31Z
started: 2026-10-12T07:55:07Z
---

## Description
`create_session` + `bind_shortcuts` (`ptt`, preferred trigger) + Activated/Deactivated signals; detect missing portal and show a fallback; remember the granted binding.

## Acceptance Criteria
- Works on KDE 6 and GNOME 48+; Hyprland documented.
