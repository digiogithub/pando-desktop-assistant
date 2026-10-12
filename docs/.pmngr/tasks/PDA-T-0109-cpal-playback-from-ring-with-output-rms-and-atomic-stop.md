---
id: PDA-T-0109
type: task
title: cpal playback from ring with output RMS and atomic stop
status: done
parent: PDA-US-0031
milestone: PDA-M-0007
assignees: [claude-sonnet]
author: mcp
labels: [voice-out, organism]
estimate: 3
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T11:03:42Z
started: 2026-10-12T07:54:44Z
closed: 2026-10-12T11:03:42Z
---

## Description
cpal output at the native rate with resampling from the voice rate; `rtrb` consumed by the callback; atomic stop flag that empties the ring (< 20 ms); RMS tap synced with what is audible → `tts_rms`; `PlaybackStarted/Drained` events.

## Acceptance Criteria
- Stop measured < 50 ms; RMS aligned ±30 ms with the audio.
