---
id: PDA-T-0131
type: task
title: Claude Code hooks installer/uninstaller
status: todo
parent: PDA-US-0039
milestone: PDA-M-0009
author: mcp
labels: [claude-bridge]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-12T07:54:49Z
---

## Description
Minimal forwarder script (sh/PowerShell) that reads port/secret and runs `curl` with a short timeout, always exiting 0; idempotent merge into `~/.claude/settings.json` with backup; verification and uninstall from settings.

## Acceptance Criteria
- Installing twice does not duplicate; uninstalling restores the user's previous hooks.
