---
id: PDA-EP-0002
type: epic
title: Technical risk spikes
status: backlog
priority: critical
milestone: PDA-M-0001
author: mcp
labels: [spike]
created: 2026-10-11T21:45:36Z
updated: 2026-10-11T22:02:27Z
---

## Description
Bounded experiments that validate the highest-risk assumptions before building: Whistle FFI + Spanish WER, Windows sidecar, single sherpa-onnx (VAD+TTS), audio + AEC, hotkeys/tray on Wayland, espeak-ng license, GPUI canvas performance, and tray/hotkey integration with the GPUI event loop.

## Acceptance Criteria
- Each spike: report in `docs/spikes/<id>.md` with measured data and a decision (ADR in `docs/adr/`).
- Risks R1–R13 in `docs/research/voice-stack.md` §8 re-evaluated.

## Notes
Refs: `docs/research/voice-stack.md` §9, `docs/research/organism-port.md` §5.
