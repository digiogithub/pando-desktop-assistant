---
id: PDA-T-0008
type: task
title: "App startup: paths, logging, atomic settings, i18n and smoke test"
status: done
parent: PDA-US-0002
milestone: PDA-M-0001
assignees: [claude-sonnet]
author: mcp
labels: [shell]
estimate: 3
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:42:42Z
started: 2026-10-11T22:42:19Z
closed: 2026-10-11T22:42:42Z
---

## Description
`directories::ProjectDirs` (`dev`, `Pando`, `pando-assistant`), `tracing` + rotating `tracing-appender`, tolerant JSON `AppSettings` with `write_atomic`, `rust-i18n` with `assets/locales/*.{es,en}.yml` (es is the source; en forced in tests), `--smoke-test` (2 frames then exit, 30 s watchdog, logs `gpu_specs`).

## Acceptance Criteria
- Corrupt settings.json ⇒ startup with defaults + backup.
- `pando-assistant --smoke-test` exits with 0.
