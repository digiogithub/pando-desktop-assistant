---
id: PDA-T-0107
type: task
title: TTS in pando-voice with sherpa-onnx (Piper + Kokoro)
status: backlog
parent: PDA-US-0031
milestone: PDA-M-0007
author: mcp
labels: [voice-out]
estimate: 5
created: 2026-10-11T21:54:29Z
updated: 2026-10-11T22:03:24Z
---

## Description
`trait Tts` and `sherpa-onnx` OfflineTts implementation in the worker; Piper voices (`es_ES-davefx-medium` by default, `es_ES-sharvard`, `es_MX`, `en_US-lessac`) and optional Kokoro; lazy loading; hot voice switching; `Synth`/`Pcm`/`SynthDone`/`Cancel` messages from `voice-proto`.

## Acceptance Criteria
- A 10-word phrase → first PCM chunk < 200 ms in the worker.
