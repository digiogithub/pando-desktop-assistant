---
id: PDA-T-0091
type: task
title: Build pando-voice for Windows x86_64-pc-windows-gnullvm
status: backlog
parent: PDA-US-0026
milestone: PDA-M-0005
author: mcp
labels: [voice-in, ci]
estimate: 3
created: 2026-10-11T21:52:55Z
updated: 2026-10-11T22:02:51Z
---

## Description
llvm-mingw toolchain in CI, target `x86_64-pc-windows-gnullvm` for the worker (the app stays MSVC), packaged next to the main exe.

## Acceptance Criteria
- End-to-end transcription on Windows from the MSVC app.
