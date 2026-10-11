---
created_at: 2026-10-11T22:18:24.062128883Z
updated_at: 2026-10-11T22:18:24.062128883Z
tags:
    - change
    - infra
---
# Workspace bootstrap (PDA-T-0001..0004)

Builds on [[pando-assistant/plans/backlog-execution_plan.md]].

## What changed
- PDA-T-0001 (ba52080): root `Cargo.toml` (resolver 3, edition 2024, rust-version 1.90, workspace deps/lints, dev opt-level 1 for deps, thin LTO release), `rust-toolchain.toml` pinned to 1.98.1, `rustfmt.toml`, `clippy.toml`, `typos.toml`, `.gitattributes`, `.cargo/config.toml` (`cargo xtask` alias), `target/` gitignored. No CI workflows (forbidden for now).
- PDA-T-0002 (b0a1958): skeleton crates `organism, decide, pando-client, voice-proto, voice-worker (bin pando-voice), claude-bridge, app (bin pando-assistant, gpui-kit =0.7.1)` plus `xtask`, each with README and a smoke test.
- PDA-T-0003 (65c6568): `xtask/src/deps.rs` (`cargo xtask check-deps`, ported from Bitacora): only `app` may reach gpui*, exact `=` gpui-kit pin, organism/decide may not reach reqwest/tokio/cpal, allowed workspace edge table; `crates/app/clippy.toml` disallows `tokio::spawn`, `Runtime::block_on`, `Handle::block_on`.
- PDA-T-0004 (dd85f13): `deny.toml` (license allow-list, `gpui` ban, 5 ignored transitive unmaintained advisories, crates.io only), `script/install-linux-deps.sh`.

## Why
Epic PDA-EP-0001 / story PDA-US-0001: a compilable workspace with lints and dependency rules.

## Verification
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace` (16 tests), `cargo build -p app` (gpui-kit builds, no extra system libs on this machine), `cargo xtask check-deps` OK and fails when reqwest is added to organism, `cargo deny check` all ok. The install script was syntax/help checked and package names resolved with apt-cache but not run (no sudo).
