---
id: PDA-T-0001
type: task
title: Create workspace Cargo.toml, toolchain, profiles and lints
status: backlog
parent: PDA-US-0001
milestone: PDA-M-0001
author: mcp
labels: [infra]
estimate: 2
created: 2026-10-11T21:47:40Z
updated: 2026-10-11T22:01:54Z
---

## Description
Root `Cargo.toml` (resolver 3, edition 2024, rust-version 1.90, `[workspace.dependencies]` with exact pins, dev profile opt-level 1 for deps and release lto thin), `rust-toolchain.toml` 1.98.1, `rustfmt.toml`, `clippy.toml`, `typos.toml`, `.gitattributes`.

## Acceptance Criteria
- `cargo fmt --check` and `cargo clippy --workspace -D warnings` pass on an empty workspace.
