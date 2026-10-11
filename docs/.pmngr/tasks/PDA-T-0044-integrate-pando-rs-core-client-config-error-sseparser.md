---
id: PDA-T-0044
type: task
title: Integrate pando-rs core (client, config, error, SseParser)
status: backlog
parent: PDA-US-0012
milestone: PDA-M-0003
author: mcp
labels: [pando-client]
estimate: 2
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:02:51Z
---

## Description
Bring from Bitácora `pando-rs` (client with two reqwest instances: normal and stream without global timeout; `PandoConfig` with extra CA; typed `Error`; `SseParser`) as the base of `crates/pando-client` (path/git dep or copy with attribution), without the AG-UI part. Header `X-Pando-Client: pando-assistant`.

## Acceptance Criteria
- Inherited tests pass; `Token` redacted in `Debug`.
