---
id: PDA-T-0120
type: task
title: global-hotkey on Windows, macOS and X11 (press/release)
status: in_progress
parent: PDA-US-0036
milestone: PDA-M-0008
assignees: [claude-sonnet]
author: mcp
labels: [shell]
estimate: 3
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T07:55:07Z
started: 2026-10-12T07:55:07Z
---

## Description
Register a configurable shortcut (per-OS defaults with no conflicts, e.g. Ctrl+Alt+Space), integrate Pressed/Released events with the GPUI loop (macOS main thread, Windows message loop), notify the voice orchestrator.

## Acceptance Criteria
- Hold-to-talk works from another app in all 3 environments.
