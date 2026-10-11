---
id: PDA-T-0054
type: task
title: "Tests: axum mock server, fixture conformance and live test"
status: backlog
parent: PDA-US-0013
milestone: PDA-M-0003
author: mcp
labels: [pando-client, testing]
estimate: 3
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:03:06Z
---

## Description
Axum mock for auth/token/401/SSE; conformance tests that reproduce fixtures byte for byte; `tests/live.rs` `#[ignore]` that starts `pando serve` and runs a real chat.

## Acceptance Criteria
- Optional CI job with Pando installed runs the live test.
