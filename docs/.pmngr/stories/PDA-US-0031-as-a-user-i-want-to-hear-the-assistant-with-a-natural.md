---
id: PDA-US-0031
type: story
title: As a user I want to hear the assistant with a natural Spanish voice and without waiting
status: backlog
priority: high
parent: PDA-EP-0010
milestone: PDA-M-0007
author: mcp
labels: [voice-out]
created: 2026-10-11T21:53:22Z
updated: 2026-10-11T22:03:09Z
---

## Description
TTS in the worker (`sherpa-onnx` + Piper `es_ES-davefx-medium`, Kokoro optional), sentence-by-sentence synthesis with pipeline, cpal playback from ring buffer with output RMS.

## Acceptance Criteria
- Implements PDA-SP-0007.R1, R2, R5.
