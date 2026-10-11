---
id: PDA-US-0026
type: story
title: As a user I want accurate local transcription, even of my project names
status: backlog
priority: high
parent: PDA-EP-0008
milestone: PDA-M-0005
author: mcp
labels: [voice-in]
created: 2026-10-11T21:51:48Z
updated: 2026-10-11T22:02:59Z
---

## Description
Worker `pando-voice` (own process) with Whistle via `needle-sys`, `voice-proto` protocol, supervisor with restart, Windows gnullvm build, dynamic keyword biasing and Parakeet (sherpa-onnx) fallback.

## Acceptance Criteria
- Implements PDA-SP-0006.R4, R5, R7.
