---
id: PDA-US-0007
type: story
title: "Platform spikes: GPUI canvas, windows, tray and hotkeys"
status: backlog
priority: high
parent: PDA-EP-0002
milestone: PDA-M-0001
author: mcp
labels: [spike, shell, organism]
created: 2026-10-11T21:46:17Z
updated: 2026-10-11T22:02:18Z
---

## Description
Measure the real cost of GPUI paths for the organism, test always-on-top/transparent windows per platform, and integrate `global-hotkey`, GlobalShortcuts portal (`ashpd`), `ksni` and `tray-icon` with the GPUI event loop.

## Acceptance Criteria
- Report with fps/CPU per frame on real GPU (not only lavapipe).
- Matrix platform × (always-on-top, hotkey press/release, tray) with result and fallback.

## Notes
Refs: `docs/research/organism-port.md` §3, `docs/research/voice-stack.md` S6, `docs/research/bitacora-gpui.md` §3.7.
