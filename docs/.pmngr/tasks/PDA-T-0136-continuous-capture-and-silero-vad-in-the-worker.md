---
id: PDA-T-0136
type: task
title: Continuous capture and Silero VAD in the worker
status: done
parent: PDA-US-0041
milestone: PDA-M-0010
assignees: [claude-sonnet]
author: mcp
labels: [wake-word, voice-in]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-12T08:13:17Z
started: 2026-10-12T08:12:36Z
closed: 2026-10-12T08:13:17Z
---

## Description
Hands-free mode: continuous 16 kHz PCM stream to the worker, Silero VAD v5 (sherpa-onnx) on 32 ms frames, segments with 300 ms pre-roll and hangover; nothing is stored except an explicit debug recording.

## Acceptance Criteria
- Idle CPU < 3 % of one core over 10 min of silence.
