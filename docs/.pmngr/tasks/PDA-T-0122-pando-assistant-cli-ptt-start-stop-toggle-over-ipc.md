---
id: PDA-T-0122
type: task
title: pando-assistant CLI ptt start|stop|toggle over IPC
status: in_review
parent: PDA-US-0036
milestone: PDA-M-0008
assignees: [claude-sonnet]
author: mcp
labels: [shell]
estimate: 2
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T08:08:31Z
started: 2026-10-12T07:55:07Z
---

## Description
Subcommand that talks to the running instance via the single-instance socket; settings show `bindsym` lines (Sway/niri) and commands for custom shortcuts on any DE.

## Acceptance Criteria
- Sway: press/release binding works with `--release`.
