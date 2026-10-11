---
id: PDA-T-0133
type: task
title: Cross-session messaging send over socket
status: backlog
parent: PDA-US-0040
milestone: PDA-M-0009
author: mcp
labels: [claude-bridge]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-11T22:02:28Z
---

## Description
Connect to the session's UDS, optional auth line, plain text with a size limit, detect close/hold ("held"), isolated in a versioned module (undocumented format). Linux/macOS only in v1.

## Acceptance Criteria
- Message delivered to a real session running Claude Code ≥ 2.1.224; errors reported to the user.
