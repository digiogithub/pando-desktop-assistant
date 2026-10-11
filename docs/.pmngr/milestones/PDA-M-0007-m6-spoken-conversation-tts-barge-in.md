---
id: PDA-M-0007
type: milestone
title: M6 · Spoken conversation (TTS + barge-in)
status: backlog
author: mcp
created: 2026-10-11T21:40:56Z
updated: 2026-10-11T22:02:15Z
due: 2027-02-19
---

## Description
Voice prompt to mesnada, spoken response/conclusion with Piper (`es_ES-davefx-medium`) via sherpa-onnx in a worker, streaming by sentence, speech queue, PTT barge-in. Organism in speaking state with output RMS.

## Acceptance Criteria
- conclusion → first audio < 300 ms.
- PTT barge-in stops TTS in < 50 ms.
- Tool output is never spoken; code blocks are read as "bloque de código omitido".

## Notes
Refs: `docs/02-voice-pipeline.md`, `docs/06-ui-mapping.md` "What is spoken".
