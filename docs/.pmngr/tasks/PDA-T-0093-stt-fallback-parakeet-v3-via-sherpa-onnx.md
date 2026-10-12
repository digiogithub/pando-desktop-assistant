---
id: PDA-T-0093
type: task
title: "STT fallback: Parakeet v3 via sherpa-onnx"
status: in_review
parent: PDA-US-0026
milestone: PDA-M-0005
assignees: [claude-sonnet]
author: mcp
labels: [voice-in]
estimate: 3
created: 2026-10-11T21:52:55Z
updated: 2026-10-12T07:55:33Z
started: 2026-10-12T07:54:44Z
---

## Description
`Stt` implementation with `sherpa-onnx` OfflineRecognizer + Silero VAD for segmentation; model downloaded with consent (model manager); automatic selection on macOS x86_64 or if Whistle fails to load.

## Acceptance Criteria
- Intel Mac transcribes after accepting the download; chip `stt · parakeet`.
