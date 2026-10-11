---
id: PDA-T-0003
type: task
title: Implement xtask check-deps and clippy disallowed-methods rules
status: in_review
parent: PDA-US-0001
milestone: PDA-M-0001
assignees: [claude-sonnet]
author: mcp
labels: [infra]
estimate: 3
created: 2026-10-11T21:47:40Z
updated: 2026-10-11T22:17:43Z
started: 2026-10-11T22:17:34Z
---

## Description
Port `xtask/src/deps.rs`: only `app` may depend on `gpui-kit`; `organism` and `decide` must not depend on `reqwest`/`tokio`/`cpal`/`gpui*`. `crates/app/clippy.toml` with `disallowed-methods` = `tokio::spawn`, `Runtime::block_on`, `Handle::block_on`.

## Acceptance Criteria
- Adding `reqwest` to `organism` makes `cargo xtask check-deps` fail.
