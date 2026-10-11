---
id: PDA-T-0093
type: task
title: "STT fallback: Parakeet v3 via sherpa-onnx"
status: backlog
parent: PDA-US-0026
milestone: PDA-M-0005
author: mcp
labels: [voice-in]
estimate: 3
created: 2026-10-11T21:52:55Z
updated: 2026-10-11T22:02:51Z
---

## Description
`Stt` implementation with `sherpa-onnx` OfflineRecognizer + Silero VAD for segmentation; model downloaded with consent (model manager); automatic selection on macOS x86_64 or if Whistle fails to load.

## Acceptance Criteria
- Intel Mac transcribes after accepting the download; chip `stt · parakeet`.
