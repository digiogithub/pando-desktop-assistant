---
id: PDA-T-0020
type: task
title: "S3 · Single sherpa-onnx: Silero VAD + Piper + Kokoro alongside libneedle"
status: backlog
parent: PDA-US-0006
milestone: PDA-M-0001
author: mcp
labels: [spike, voice-out]
estimate: 2
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:02:23Z
---

## Description
One binary with statically linked `sherpa-onnx` 1.13.8 (VAD, Piper `es_ES-davefx-medium`, Kokoro es) and libneedle, on Linux x86_64/aarch64, macOS arm64 and Windows. Measure time to first audio for a 10-word sentence; assess Piper vs Kokoro quality in Spanish (informal MOS).

## Acceptance Criteria
- Time to first audio under 300 ms; no libc++/libstdc++ conflicts or double ORT.
