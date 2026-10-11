---
id: PDA-T-0021
type: task
title: S4 · Audio with cpal 0.18 + rubato 5 + sonora AEC
status: in_review
parent: PDA-US-0006
milestone: PDA-M-0001
assignees: [claude-opus]
author: mcp
labels: [spike, voice-in, voice-out]
estimate: 3
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:43:09Z
started: 2026-10-11T22:22:20Z
---

## Description
Capture and playback on PipeWire, Pulse, CoreAudio and WASAPI; FFT resampling 48→16 kHz; AEC3 from `sonora` with TTS reference; barge-in detection with speaker playback.

## Acceptance Criteria
- ERLE measured; 0 false barge-ins in 5 min of TTS over speakers; device hot-plug recovered.
