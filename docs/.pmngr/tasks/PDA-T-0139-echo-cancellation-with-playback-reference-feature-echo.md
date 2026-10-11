---
id: PDA-T-0139
type: task
title: Echo cancellation with playback reference (feature echo-cancel)
status: backlog
parent: PDA-US-0042
milestone: PDA-M-0010
author: mcp
labels: [wake-word, voice-out]
estimate: 5
created: 2026-10-11T21:56:03Z
updated: 2026-10-11T22:02:45Z
---

## Description
Pass the exact PCM being played as render reference in 10 ms frames aligned with capture; AEC3 + NS before VAD/wake; headphone detection to disable; trait to switch to `webrtc-audio-processing`.

## Acceptance Criteria
- ERLE ≥ spike S4 target; no false triggers with 5 min of TTS through speakers.
