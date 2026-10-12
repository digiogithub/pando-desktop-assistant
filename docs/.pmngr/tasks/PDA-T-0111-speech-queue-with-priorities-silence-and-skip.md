---
id: PDA-T-0111
type: task
title: Speech queue with priorities, silence and skip
status: done
parent: PDA-US-0032
milestone: PDA-M-0007
assignees: [claude-sonnet]
author: mcp
labels: [voice-out]
estimate: 3
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T07:55:39Z
started: 2026-10-12T07:54:44Z
closed: 2026-10-12T07:55:39Z
---

## Description
`SpeechQueue`: priority direct answer > requested conclusion > notification; waits if the body is in Listening; mute (visible text, cell pulse), skip ("calla", Esc), no overlaps; "tengo algo" signal (pulse) for pending conclusions.

## Acceptance Criteria
- Tests for ordering and for waiting during Listening.
