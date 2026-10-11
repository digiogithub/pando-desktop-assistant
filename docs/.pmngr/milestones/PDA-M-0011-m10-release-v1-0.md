---
id: PDA-M-0011
type: milestone
title: M10 · Release v1.0
status: backlog
author: mcp
created: 2026-10-11T21:40:56Z
updated: 2026-10-11T22:02:15Z
due: 2027-04-30
---

## Description
Packaging (deb, AppImage, Flatpak, signed and notarized dmg, signed NSIS/MSI), auto-update (velopack), model manager with verified downloads, license notices, user documentation and release CI.

## Acceptance Criteria
- Install → first use offline (Whistle + VAD + 1 voice included) on all 3 platforms.
- Tag-based release publishes artifacts + sha256sums.
- Complete NOTICE/licenses (Apache-2.0 needle, CC0 voice, GPL sidecar TTS if applicable).

## Notes
Refs: `docs/research/bitacora-gpui.md` §4, `docs/research/voice-stack.md` §6.
