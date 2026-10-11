---
id: PDA-T-0169
type: task
title: G15 · Mutex in token rotation and optional persistence
status: cancelled
priority: medium
parent: PDA-US-0049
milestone: PDA-M-0003
author: mcp
labels: [upstream-pando, security]
estimate: 1
created: 2026-10-11T21:57:00Z
updated: 2026-10-11T22:21:30Z
closed: 2026-10-11T22:21:30Z
---

## Description
Protect `s.token` with a mutex (`handlers_extras.go:64`), unify length, optional persistence in `~/.config/pando/api-token` (0600) like the mcp/agui tokens.

## Acceptance Criteria
- Race test (`-race`) clean.
