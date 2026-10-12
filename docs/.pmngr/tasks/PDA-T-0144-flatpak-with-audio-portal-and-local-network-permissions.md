---
id: PDA-T-0144
type: task
title: Flatpak with audio, portal and local network permissions
status: in_review
parent: PDA-US-0043
milestone: PDA-M-0011
assignees: [claude-sonnet]
author: mcp
labels: [packaging]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-12T08:15:14Z
started: 2026-10-12T08:01:03Z
---

## Description
Manifest and metainfo; permissions: pulseaudio/pipewire, network (loopback to Pando), read access to `~/.config/pando/tls` and `/tmp/pando-instances`, GlobalShortcuts and Background portals, `~/.claude` for hooks (optional).

## Acceptance Criteria
- Works on Fedora and Ubuntu via a Flathub-like build.
