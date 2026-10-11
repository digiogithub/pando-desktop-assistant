---
id: PDA-T-0124
type: task
title: Single instance and background mode
status: backlog
parent: PDA-US-0037
milestone: PDA-M-0008
author: mcp
labels: [shell]
estimate: 2
created: 2026-10-11T21:54:29Z
updated: 2026-10-11T22:02:08Z
---

## Description
Exclusive lock + IPC socket (forward args and focus); keep the process alive when the last window closes if the tray is present (Global `KeepRunning`); orderly shutdown (deny pending, stop worker, close streams).

## Acceptance Criteria
- Second launch focuses the existing window.
