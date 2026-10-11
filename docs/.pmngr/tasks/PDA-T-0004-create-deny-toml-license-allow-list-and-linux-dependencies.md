---
id: PDA-T-0004
type: task
title: Create deny.toml license allow-list and Linux dependencies script
status: in_progress
parent: PDA-US-0001
milestone: PDA-M-0001
assignees: [claude-sonnet]
author: mcp
labels: [infra]
estimate: 1
created: 2026-10-11T21:47:40Z
updated: 2026-10-11T22:17:53Z
started: 2026-10-11T22:17:53Z
---

## Description
`deny.toml` (blocks GPL in the `app` dependency graph; documented exception for `voice-worker` only if decided), `script/install-linux-deps.sh` (Vulkan, xkbcommon, wayland, alsa/pipewire dev, clang/libc++ for libneedle).

## Acceptance Criteria
- `cargo deny check` passes; script tested on Ubuntu 24.04.
