---
id: PDA-T-0087
type: task
title: "voice-proto: app ↔ pando-voice protocol"
status: in_progress
parent: PDA-US-0026
milestone: PDA-M-0005
assignees: [claude-sonnet]
author: mcp
labels: [voice-in]
estimate: 2
created: 2026-10-11T21:52:55Z
updated: 2026-10-12T07:26:26Z
started: 2026-10-12T07:26:26Z
---

## Description
Length-prefixed messages over stdio (bincode or postcard): `Hello{version}`, `LoadStt{model, lang}`, `Transcribe{id, pcm, lang, keywords, word_ts}`, `StreamChunk/StreamStop`, `Synth{id, text, voice}` → `Pcm{id, chunk}`/`SynthDone`, `Cancel{id}`, `Error`, `Health`. Versioned, with round-trip tests.

## Acceptance Criteria
- Basic decoder fuzzing without panics.
