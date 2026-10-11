---
id: PDA-US-0006
type: story
title: "Voice spikes: STT, TTS, audio and licensing"
status: backlog
priority: critical
parent: PDA-EP-0002
milestone: PDA-M-0001
author: mcp
labels: [spike, voice-in, voice-out]
created: 2026-10-11T21:46:17Z
updated: 2026-10-11T22:02:18Z
---

## Description
Validate the voice stack before building: Whistle FFI and Spanish WER, Windows sidecar, sherpa-onnx as the single ORT (VAD + Piper + Kokoro) alongside libneedle, cpal 0.18 + rubato 5 + sonora AEC, wake word comparison, GPL espeak-ng decision.

## Acceptance Criteria
- Reports S1–S5, S7 in `docs/spikes/` with measured numbers and a decision.
- Explicit decision: `pando-voice` worker on all OSes or only Windows.

## Notes
Refs: `docs/research/voice-stack.md` §9 (S1–S7).
