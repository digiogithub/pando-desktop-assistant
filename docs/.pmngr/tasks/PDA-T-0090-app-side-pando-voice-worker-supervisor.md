---
id: PDA-T-0090
type: task
title: App-side pando-voice worker supervisor
status: done
parent: PDA-US-0026
milestone: PDA-M-0005
assignees: [claude-sonnet]
author: mcp
labels: [voice-in]
estimate: 3
created: 2026-10-11T21:52:55Z
updated: 2026-10-12T11:03:42Z
started: 2026-10-12T08:01:42Z
closed: 2026-10-12T11:03:42Z
---

## Description
Launch `pando-voice` alongside the binary, version handshake, request queue with ids and cancellation, restart with backoff on crash, "voz disponible/no disponible" status in the header (`stt · whistle`), orderly shutdown on exit.

## Acceptance Criteria
- Killing the worker mid-transcription ⇒ visible error and automatic recovery.
