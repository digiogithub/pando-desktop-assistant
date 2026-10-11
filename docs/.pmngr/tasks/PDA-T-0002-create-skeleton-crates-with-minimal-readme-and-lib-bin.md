---
id: PDA-T-0002
type: task
title: Create skeleton crates with minimal README and lib/bin
status: in_progress
parent: PDA-US-0001
milestone: PDA-M-0001
assignees: [claude-sonnet]
author: mcp
labels: [infra]
estimate: 2
created: 2026-10-11T21:47:40Z
updated: 2026-10-11T22:16:25Z
started: 2026-10-11T22:16:25Z
---

## Description
`crates/{organism,decide,pando-client,voice-proto,voice-worker,claude-bridge,app}` + `xtask`. `voice-worker` produces bin `pando-voice`; `app` produces bin `pando-assistant`. Each crate has a purpose doc and a trivial test.

## Acceptance Criteria
- `cargo test --workspace` runs at least 1 test per crate.
