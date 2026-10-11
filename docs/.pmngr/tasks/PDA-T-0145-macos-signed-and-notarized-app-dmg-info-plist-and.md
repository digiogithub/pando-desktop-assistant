---
id: PDA-T-0145
type: task
title: "macOS: signed and notarized .app/.dmg, Info.plist and entitlements"
status: backlog
parent: PDA-US-0043
milestone: PDA-M-0011
author: mcp
labels: [packaging, security]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-11T22:02:45Z
---

## Description
`NSMicrophoneUsageDescription`, `com.apple.security.device.audio-input`, hardened runtime, worker signing, notarization (secrets via env), arm64 (x86_64 with STT fallback).

## Acceptance Criteria
- Gatekeeper opens the app without warnings; microphone permission requested correctly.
