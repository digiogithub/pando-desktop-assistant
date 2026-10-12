---
id: PDA-T-0078
type: task
title: "Usage panel: tokens, cache, reasoning and cost"
status: done
parent: PDA-US-0022
milestone: PDA-M-0004
assignees: [claude-sonnet]
author: mcp
labels: [chat]
estimate: 2
created: 2026-10-11T21:51:27Z
updated: 2026-10-12T08:20:17Z
started: 2026-10-11T22:46:56Z
closed: 2026-10-12T08:20:17Z
---

## Description
Breakdown of `token_usage` (prompt, completion, cache read/creation, reasoning, cost), marking estimated vs confirmed values; session totals from `Session` (PascalCase).

## Acceptance Criteria
- No fields are dropped (WebUI parser bug).
