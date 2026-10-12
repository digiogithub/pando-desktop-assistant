---
id: PDA-T-0114
type: task
title: Barge-in via PTT
status: done
parent: PDA-US-0033
milestone: PDA-M-0007
assignees: [claude-sonnet]
author: mcp
labels: [voice-out, voice-in]
estimate: 2
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T11:03:42Z
started: 2026-10-12T08:47:05Z
closed: 2026-10-12T11:03:42Z
---

## Description
Pressing PTT (window, global or mini) while speaking: stop in < 50 ms, discard the pending sentences of that utterance, switch to Listening; if the run is still active, the new prompt goes in as steering.

## Acceptance Criteria
- Scenario of PDA-SP-0007.R6 measured.
