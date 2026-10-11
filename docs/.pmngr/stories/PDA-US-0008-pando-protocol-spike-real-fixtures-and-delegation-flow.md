---
id: PDA-US-0008
type: story
title: "Pando protocol spike: real fixtures and delegation flow"
status: backlog
priority: high
parent: PDA-EP-0002
milestone: PDA-M-0001
author: mcp
labels: [spike, pando-client]
created: 2026-10-11T21:46:17Z
updated: 2026-10-11T22:02:18Z
---

## Description
Record real Pando SSE streams (chat with tools, permissions, questions, steering, goal, delegation + resurrection, slash commands) and document the `run_seq` flow and the `[delegated-result]` injection.

## Acceptance Criteria
- Fixtures in `crates/pando-client/tests/fixtures/` with the Pando commit used.
- Sequence diagram of the delegation → conclusion → resurrection flow.

## Notes
Refs: `docs/research/pando-api.md` §3.3–3.5.
