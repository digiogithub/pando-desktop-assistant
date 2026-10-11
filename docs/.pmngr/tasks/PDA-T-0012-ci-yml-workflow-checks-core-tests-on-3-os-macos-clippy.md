---
id: PDA-T-0012
type: task
title: "ci.yml workflow: checks, core tests on 3 OS, macOS clippy"
status: backlog
parent: PDA-US-0004
milestone: PDA-M-0001
author: mcp
labels: [ci]
estimate: 3
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:02:09Z
---

## Description
Jobs: changes filter; checks (fmt, typos, machete, deny, check-deps, tokens --check); test-core on ubuntu/macos/windows with `setup-rust-toolchain` + shared cache; clippy `-D warnings` on macos-latest.

## Acceptance Criteria
- Total time under 15 min with a warm cache.
