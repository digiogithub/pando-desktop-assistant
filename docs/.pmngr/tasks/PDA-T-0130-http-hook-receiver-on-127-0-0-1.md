---
id: PDA-T-0130
type: task
title: HTTP hook receiver on 127.0.0.1
status: done
parent: PDA-US-0039
milestone: PDA-M-0009
assignees: [claude-sonnet]
author: mcp
labels: [claude-bridge, security]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-12T07:59:38Z
started: 2026-10-12T07:59:17Z
closed: 2026-10-12T07:59:38Z
---

## Description
Axum server on loopback, random port + shared secret written to a 0600 runtime file; endpoints `/claude/hook` for SessionStart, UserPromptSubmit, PreToolUse, Notification, Stop, SessionEnd; size validation and secret header check; events forwarded to the cell source.

## Acceptance Criteria
- Requests without the secret ⇒ 401; oversized payloads rejected.
