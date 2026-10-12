---
id: PDA-T-0079
type: task
title: Permissions dialog with queue and escalation variant
status: in_review
parent: PDA-US-0023
milestone: PDA-M-0004
assignees: [claude-sonnet]
author: mcp
labels: [chat, safety]
estimate: 3
created: 2026-10-11T21:51:27Z
updated: 2026-10-12T08:10:58Z
started: 2026-10-12T08:06:42Z
---

## Description
Modal with the first pending item and "+N en cola": tool, description, path, params; "Permitir" / "Permitir en sesión" / "Denegar" (keyboard shortcuts); `execute_unsandboxed` variant with command, justification and `grant_key` scope (`prefix:` / `exact:`). Replay of pending items on reattach.

## Acceptance Criteria
- Scenarios of PDA-SP-0003.R1.
