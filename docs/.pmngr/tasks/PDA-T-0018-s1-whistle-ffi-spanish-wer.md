---
id: PDA-T-0018
type: task
title: S1 · Whistle FFI + Spanish WER
status: in_review
parent: PDA-US-0006
milestone: PDA-M-0001
assignees: [claude-opus]
author: mcp
labels: [spike, voice-in]
estimate: 3
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T23:22:35Z
started: 2026-10-11T22:26:51Z
---

## Description
`needle-sys` (hand-written bindings from the header) + safe wrapper on linux-x86_64 and macos-arm64, statically linking libc++/libc++abi. WER is measured on Common Voice (200 clips), FLEURS es and 50 own commands with project-name bias; compare with Parakeet v3 (sherpa) and Whisper small. Latency P50/P95 3 s/10 s.

## Acceptance Criteria
- `docs/spikes/S1-whistle.md` with tables and a go/no-go recommendation.

## Notes
Risks R1, R4, R5. SHA pins in `docs/research/voice-stack.md` §6.2.
