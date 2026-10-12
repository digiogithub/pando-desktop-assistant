---
id: PDA-T-0132
type: task
title: ClaudeExternal cells, project by cwd and "avísame"
status: in_progress
parent: PDA-US-0039
milestone: PDA-M-0009
assignees: [claude-sonnet]
author: mcp
labels: [claude-bridge, organism]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-12T07:59:17Z
started: 2026-10-12T07:59:17Z
---

## Description
Map hooks to state (born empty blue, working, waiting `?`, done with the Stop summary, dies on SessionEnd or after inactivity), assign project by comparing cwd with `/projects`, store messaging socket/token, "avísame al terminar" flag that queues the summary to be spoken.

## Acceptance Criteria
- Scenario from PDA-SP-0010.R1 in < 500 ms.
