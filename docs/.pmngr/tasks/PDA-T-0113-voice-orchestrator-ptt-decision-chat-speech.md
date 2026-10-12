---
id: PDA-T-0113
type: task
title: "Voice orchestrator: PTT → decision → chat → speech"
status: in_review
parent: PDA-US-0033
milestone: PDA-M-0007
assignees: [claude-sonnet]
author: mcp
labels: [voice-out, voice-in]
estimate: 5
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T08:47:05Z
started: 2026-10-12T08:47:05Z
---

## Description
State machine of the voice session connecting capture, STT, decision, chat stream to the current target, and TTS; handles body states (Listening/Thinking/Speaking/Idle; Thinking ends at the first non-empty `content_delta`); errors at each stage with a short spoken message and cancellation.

## Acceptance Criteria
- The transition table in `docs/06` is covered by tests with doubles for each stage.
