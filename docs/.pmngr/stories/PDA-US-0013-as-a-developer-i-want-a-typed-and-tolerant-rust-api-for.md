---
id: PDA-US-0013
type: story
title: As a developer I want a typed and tolerant Rust API for everything the assistant uses
status: backlog
priority: critical
parent: PDA-EP-0004
milestone: PDA-M-0003
author: mcp
labels: [pando-client]
created: 2026-10-11T21:49:06Z
updated: 2026-10-11T22:02:31Z
---

## Description
Generic SSE with reconnection; types and calls for sessions/messages, chat stream, orchestrator, instances, projects (+ proxy), models, commands, notifications, decision model; `[delegated-result]` parser; tests with mock and real fixtures.

## Acceptance Criteria
- Implements PDA-SP-0001.R5, R6.
- All fixtures from the protocol spike parse without unexpected `Unknown` values.

## Notes
Mixed casing: serde per type with `#[serde(default)]` everywhere. Refs: `docs/research/pando-api.md` §2–§3.
