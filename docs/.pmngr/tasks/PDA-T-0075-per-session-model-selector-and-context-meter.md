---
id: PDA-T-0075
type: task
title: Per-session model selector and context meter
status: in_progress
parent: PDA-US-0021
milestone: PDA-M-0004
assignees: [claude-sonnet]
author: mcp
labels: [chat]
estimate: 3
created: 2026-10-11T21:51:27Z
updated: 2026-10-12T08:06:42Z
started: 2026-10-12T08:06:42Z
---

## Description
Searchable popover grouped by provider, `auto` first with router health, badges; `PUT /models/active` always with `sessionId`; context meter from `token_usage` (warning at 70 %, danger at 90 %).

## Acceptance Criteria
- The global model is never changed from the assistant.
