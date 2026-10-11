---
id: PDA-US-0012
type: story
title: As a user I want the assistant to connect automatically to my local Pando
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
Discovery via instance registry, `/health` probe, local CA, ephemeral token, retry on 401, remote Basic auth, connection service with states and backoff, version gate.

## Acceptance Criteria
- Implements PDA-SP-0001.R1–R4.
- Starting with `pando serve`, `pando app` or `pando desktop` running connects without any configuration.

## Notes
Base: `crates/pando-rs` (client/config/error) and `bitacora-pando` (`credentials.rs`, `connection.rs`, `service.rs`).
