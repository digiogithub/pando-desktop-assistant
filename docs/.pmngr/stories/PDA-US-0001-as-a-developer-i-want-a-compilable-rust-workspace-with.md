---
id: PDA-US-0001
type: story
title: As a developer I want a compilable Rust workspace with lints and dependency rules
status: backlog
priority: critical
parent: PDA-EP-0001
milestone: PDA-M-0001
author: mcp
labels: [infra]
created: 2026-10-11T21:46:17Z
updated: 2026-10-11T22:02:01Z
---

## Description
Workspace `resolver = "3"`, edition 2024, toolchain 1.98.1 (MSRV 1.90), Bitácora profiles and lints, skeleton crates: `organism`, `decide`, `pando-client`, `voice-proto`, `voice-worker` (bin `pando-voice`), `claude-bridge`, `app` (bin `pando-assistant`), `xtask`.

## Acceptance Criteria
- `cargo build --workspace` and `cargo test --workspace` pass.
- `cargo xtask check-deps` fails if any crate other than `app` depends on `gpui-kit`, or if `organism`/`decide` depend on network/audio/UI.
- clippy `unwrap_used/expect_used/dbg_macro = deny`; `tokio::spawn`/`block_on` forbidden in `app`.

## Notes
Copy from Bitácora: root `Cargo.toml`, `rust-toolchain.toml`, `clippy.toml`, `deny.toml`, `typos.toml`, `rustfmt.toml`, `xtask/src/deps.rs`.
