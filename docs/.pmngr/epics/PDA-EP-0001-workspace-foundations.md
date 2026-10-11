---
id: PDA-EP-0001
type: epic
title: Workspace foundations
status: backlog
priority: critical
milestone: PDA-M-0001
author: mcp
labels: [infra]
created: 2026-10-11T21:45:36Z
updated: 2026-10-11T22:02:27Z
---

## Description
Rust 2024 workspace with crates `organism`, `decide`, `pando-client`, `voice-proto`, `voice-worker` (`pando-voice`), `claude-bridge`, `app` and `xtask`; toolchain, lints, CI and GPUI facade copied from the Bitácora pattern. Design system tokens generated to Rust.

## Acceptance Criteria
- CI green (fmt, clippy -D warnings on macOS, core tests on 3 OS, cargo-deny, check-deps, tokens --check).
- `cargo run -p app` opens a GPUI window with Pando fonts and palette.
- Dependency rule: only `app` depends on `gpui-kit`; `organism`/`decide` have no network or UI dependencies.

## Notes
Refs: `docs/research/bitacora-gpui.md` §1, §3, §4, §6. Decisions: rust-i18n (not Fluent); GPUI via `gpui-kit =0.7.1`.
