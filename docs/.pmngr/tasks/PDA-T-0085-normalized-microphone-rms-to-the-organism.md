---
id: PDA-T-0085
type: task
title: Normalized microphone RMS to the organism
status: backlog
parent: PDA-US-0025
milestone: PDA-M-0005
author: mcp
labels: [voice-in, organism]
estimate: 2
created: 2026-10-11T21:52:55Z
updated: 2026-10-11T22:02:51Z
---

## Description
RMS per 20 ms, slow-tracking peak + noise gate → 0..1, published at ≥ 50 Hz in an `AtomicU32`/watch read by `OrganismView` as `mic_rms`.

## Acceptance Criteria
- Speaking softly vs loudly is visually distinguishable; silence ≈ 0.
