---
id: PDA-T-0084
type: task
title: "Audio capture: cpal 0.18 → rtrb → rubato 5.x → 16 kHz mono"
status: backlog
parent: PDA-US-0025
milestone: PDA-M-0005
author: mcp
labels: [voice-in]
estimate: 5
created: 2026-10-11T21:52:55Z
updated: 2026-10-11T22:03:38Z
---

## Description
`audio::capture` module in `app` (or crate `audio`): default device or the chosen one at its native rate; allocation- and lock-free callback into `rtrb`; worker thread with `rubato` (fixed FFT 48→16 k; sinc for uncommon rates); downmix to mono; 2 s pre-roll ring; rebuild on `DeviceChanged/StreamInvalidated`; PipeWire/Pulse/ALSA host selection in settings.

## Acceptance Criteria
- Bluetooth headphone hot-plug recovered in < 1 s; no xruns in 10 min.
