---
id: PDA-EP-0008
type: epic
title: Voice input and STT
status: backlog
priority: high
milestone: PDA-M-0005
author: mcp
labels: [voice-in]
created: 2026-10-11T21:45:36Z
updated: 2026-10-11T22:02:43Z
---

## Description
Capture, resampling, RMS, push-to-talk, `pando-voice` worker with Whistle (FFI `needle-sys`), keyword biasing, sherpa-onnx Parakeet fallback, microphone permissions and WER benchmark.

## Acceptance Criteria
- PDA-SP-0006.R1–R8 verified on Linux, macOS arm64 and Windows.

## Notes
Spec: PDA-SP-0006. Refs: `docs/research/voice-stack.md` §1, §4. Corrects `docs/02-voice-pipeline.md` (libc++, needle_load by bytes, no depth, streaming available).
