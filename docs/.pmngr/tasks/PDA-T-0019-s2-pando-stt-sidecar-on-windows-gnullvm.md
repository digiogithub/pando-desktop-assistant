---
id: PDA-T-0019
type: task
title: S2 · pando-stt sidecar on Windows (gnullvm)
status: backlog
parent: PDA-US-0006
milestone: PDA-M-0001
author: mcp
labels: [spike, voice-in]
estimate: 2
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:02:23Z
---

## Description
Build a Rust binary for `x86_64-pc-windows-gnullvm` that links `libneedle.a` (GNU ABI) and talks over stdio with an MSVC host (length-prefixed PCM → JSON protocol).

## Acceptance Criteria
- Round-trip of a 5 s clip with less than 20 ms of IPC overhead.
