---
id: PDA-US-0046
type: story
title: As the assistant I need complete events and data from the mesnada agents
status: backlog
priority: high
parent: PDA-EP-0015
milestone: PDA-M-0003
author: mcp
labels: [upstream-pando]
created: 2026-10-11T21:56:18Z
updated: 2026-10-11T22:03:36Z
---

## Description
Gaps G1 (orchestrator stream), G2 (full TaskResponse), G3 (structured conclusion), G4 (pause/resume HTTP), G12 (cross-session pending items). Implemented in `/www/MCP/Pando/pando`.

## Acceptance Criteria
- Additive changes with tests in Pando; the assistant detects them and stops polling/heuristics.

## Notes
Refs: `docs/research/pando-api.md` §6.2.
