---
id: PDA-T-0022
type: task
title: S5 · Wake word comparison for "Oye Pando"
status: backlog
parent: PDA-US-0006
milestone: PDA-M-0001
author: mcp
labels: [spike, wake-word]
estimate: 3
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:02:23Z
---

## Description
Compare VAD→Whistle with bias, sherpa KWS with English tokens, and a custom openWakeWord model (`livekit-wakeword`/`oww-rs`). Record a dataset (3 h of office/podcast noise plus positive samples at 0.5/1/2 m).

## Acceptance Criteria
- FAR/FRR and idle CPU per option; recommendation for M9.
