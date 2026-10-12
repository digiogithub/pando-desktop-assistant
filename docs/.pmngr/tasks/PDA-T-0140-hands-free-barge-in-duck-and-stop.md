---
id: PDA-T-0140
type: task
title: "Hands-free barge-in: duck and stop"
status: in_review
parent: PDA-US-0042
milestone: PDA-M-0010
assignees: [claude-sonnet]
author: mcp
labels: [wake-word, voice-out]
estimate: 2
created: 2026-10-11T21:56:03Z
updated: 2026-10-12T08:47:06Z
started: 2026-10-12T08:47:06Z
---

## Description
During TTS: raised VAD threshold on clean signal; ≥ 200 ms of voice ⇒ lower output by 12 dB; continued voice for 400 ms ⇒ stop and capture the turn reusing the pre-roll.

## Acceptance Criteria
- Interrupting by speaking works with speakers and AEC active.
