---
id: PDA-T-0143
type: task
title: "cargo-packager: deb and AppImage (x86_64 + aarch64)"
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
`[package.metadata.packager]` (identifier `dev.pando.assistant`, icons, OFL fonts as resources, `.desktop` with `StartupWMClass`, depends Vulkan/xkbcommon/wayland/alsa/pipewire), `pando-voice` + bundled models; `cargo xtask bundle`.

## Acceptance Criteria
- Clean install on Ubuntu 24.04 and Debian 12 starts and transcribes offline.
