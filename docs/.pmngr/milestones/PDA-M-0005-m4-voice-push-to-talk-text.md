---
id: PDA-M-0005
type: milestone
title: "M4 · Voice: push-to-talk → text"
status: backlog
author: mcp
created: 2026-10-11T21:40:56Z
updated: 2026-10-11T22:01:59Z
due: 2027-01-15
---

## Description
Capture `cpal` → `rubato` 16 kHz → Whistle (worker `pando-voice`) with keyword biasing (projects + agents). Push-to-talk in window and global. The organism listens with real RMS.

## Acceptance Criteria
- Releasing the key → transcript < 150 ms for ≤ 10 s of audio (mid-range x86, P50).
- Spanish WER measured and documented; project and agent names correct ≥ 90 % on our own set.
- Works on Linux x86_64/aarch64, macOS arm64 and Windows x86_64 (sidecar); macOS Intel uses a fallback.

## Notes
Refs: `docs/research/voice-stack.md` §1, §4.
