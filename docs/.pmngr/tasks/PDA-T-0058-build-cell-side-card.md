---
id: PDA-T-0058
type: task
title: Build cell side card
status: done
parent: PDA-US-0015
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [shell]
estimate: 3
created: 2026-10-11T21:50:04Z
updated: 2026-10-12T07:53:44Z
started: 2026-10-12T07:53:19Z
closed: 2026-10-12T07:53:44Z
---

## Description
Side panel (max. 500 px) with a card: label, engine, status (text + icon, not color alone), project (path match when G2 provides `work_dir`), current tool, last 3 output lines, conclusion (status/confidence) and a "Ver salida completa" button (monospace view with scroll).

## Acceptance Criteria
- Updates live without flicker while the task runs.
