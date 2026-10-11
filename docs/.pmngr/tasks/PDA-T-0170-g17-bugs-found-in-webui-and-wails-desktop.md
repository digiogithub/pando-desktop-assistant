---
id: PDA-T-0170
type: task
title: G17 · Bugs found in WebUI and Wails desktop
status: backlog
priority: medium
parent: PDA-US-0049
milestone: PDA-M-0003
author: mcp
labels: [upstream-pando, bug]
estimate: 2
created: 2026-10-11T21:57:00Z
updated: 2026-10-11T22:03:25Z
---

## Description
1) `CreateTaskDialog` sends `{name, description}` instead of `prompt` (400). 2) Desktop notifier without token ⇒ 401 loop (`internal/desktop/notifications.go:58-63`). 3) WebUI SSE parser drops the `token_usage` breakdown (`sse.ts:234-242`). 4) `GetServerInfo` binding and `desktop/README.md` outdated. 5) HTTP SDKs with Bearer and non-existent endpoints.

## Acceptance Criteria
- Each bug has a test or a manually documented verification in Pando.
