---
id: PDA-EP-0004
type: epic
title: Pando client (pando-client)
status: backlog
priority: critical
milestone: PDA-M-0003
author: mcp
labels: [pando-client]
created: 2026-10-11T21:45:36Z
updated: 2026-10-11T22:02:27Z
---

## Description
Typed Rust SDK for Pando's REST + SSE API: discovery, TLS, token, connection state, generic SSE with reconnection, and APIs for sessions, chat, orchestrator, projects, instances, models, commands, notifications and the decision model. Base: `crates/pando-rs` from Bitácora (client, SseParser, errors, config).

## Acceptance Criteria
- PDA-SP-0001.R1–R6 verified.
- Tests with a mock axum server and SSE fixtures recorded from real Pando; live test `#[ignore]` against `pando serve`.

## Notes
Spec: PDA-SP-0001. Decision: REST protocol `chat/stream` (not AG-UI). Refs: `docs/research/pando-api.md`.
