---
id: PDA-T-0089
type: task
title: Whistle wrapper + Stt trait in the worker
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
`Whistle::load(bytes)`, `transcribe(pcm, opts) -> Transcript{text, lang, words, ttft_ms}` parsing the JSON from the buffer (256 KiB), streaming `stream_process/stop` for live captions, single-instance guard; common `trait Stt` shared with the fallback.

## Acceptance Criteria
- A test with a Spanish fixture WAV returns the expected text.
