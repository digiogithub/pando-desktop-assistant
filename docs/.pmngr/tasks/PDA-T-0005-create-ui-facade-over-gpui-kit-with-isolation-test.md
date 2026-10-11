---
id: PDA-T-0005
type: task
title: Create ui/ facade over gpui-kit with isolation test
status: backlog
parent: PDA-US-0002
milestone: PDA-M-0001
author: mcp
labels: [shell]
estimate: 3
created: 2026-10-11T21:47:40Z
updated: 2026-10-11T22:01:54Z
---

## Description
`crates/app/src/ui/mod.rs` with explicit re-exports (no glob), `application()`, `init()`, `open_main_window()`, `notify()`, `confirm()`, `key_binding()`, `canvas` module; test `only_ui_names_gpui_kit`.

## Acceptance Criteria
- Using `gpui_kit::` outside `ui/` breaks the test.

## Notes
Base: Bitácora `crates/bitacora-app/src/ui/mod.rs`.
