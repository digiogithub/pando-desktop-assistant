---
id: PDA-M-0004
type: milestone
title: M3 · Native chat client
status: backlog
author: mcp
created: 2026-10-11T21:40:56Z
updated: 2026-10-11T22:01:59Z
due: 2026-12-18
---

## Description
Full chat against `/api/v1/chat/stream`: sessions, streaming markdown, thinking, tool cards with diffs, permissions and questions, steering, cancel, plan/todos/goal, token usage, model selector, slash commands, reattach via `run_seq`.

## Acceptance Criteria
- Functional parity with the WebUI chat (checklist §7.2–7.7 in `docs/research/pando-api.md`).
- Reattach after closing/reopening the app during a run without losing the response.
- Sessions revived by mesnada (subagent completion) appear without intervention (`run_seq` polling or G5).

## Notes
Decision: REST protocol `chat/stream` (not AG-UI). Reuse the `pando-rs` core from Bitácora.
