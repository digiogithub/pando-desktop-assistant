---
id: PDA-T-0086
type: task
title: Microphone permissions and device selector
status: backlog
parent: PDA-US-0025
milestone: PDA-M-0005
author: mcp
labels: [voice-in, shell]
estimate: 2
created: 2026-10-11T21:52:55Z
updated: 2026-10-11T22:02:51Z
---

## Description
Detect `PermissionDenied` (macOS TCC, Windows privacy settings, portal) and show a sheet with a deep link; settings with a list of inputs/outputs and a test level meter.

## Acceptance Criteria
- Denying the permission does not crash the app; granting it again works without restart (where the OS allows it).
