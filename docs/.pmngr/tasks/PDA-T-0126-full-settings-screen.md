---
id: PDA-T-0126
type: task
title: Full settings screen
status: in_review
parent: PDA-US-0038
milestone: PDA-M-0008
assignees: [claude-sonnet]
author: mcp
labels: [shell]
estimate: 5
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T08:19:06Z
started: 2026-10-12T08:19:06Z
---

## Description
Sections: "Conexión" (URL, remote credentials), "Voz" (STT engine, language, TTS voice, speed, mute, hands-free, sensitivity), "Audio" (host, devices, test), "Atajos", "Apariencia" (layout, reduced motion), "Sistema" (autostart, tray), "Avanzado" (open Pando WebUI, logs, decisions). Secrets only in keychain.

## Acceptance Criteria
- Each setting applies without restart except those marked.
