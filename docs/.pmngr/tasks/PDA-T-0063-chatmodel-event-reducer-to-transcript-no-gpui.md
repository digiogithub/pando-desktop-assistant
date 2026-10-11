---
id: PDA-T-0063
type: task
title: "ChatModel: event reducer to transcript (no GPUI)"
status: done
parent: PDA-US-0017
milestone: PDA-M-0004
assignees: [claude-sonnet]
author: mcp
labels: [chat]
estimate: 5
created: 2026-10-11T21:51:26Z
updated: 2026-10-11T22:47:17Z
started: 2026-10-11T22:46:56Z
closed: 2026-10-11T22:47:17Z
---

## Description
GPUI-free module (in `pando-client::chat` or crate `chat-core`) inspired by `ChatModel::apply` in Bitácora: messages, reasoning, tool calls (input/result/duration), approvals, notices, plan/todos/goal/usage, live bubble with reset on replay; session generation counter to discard late events.

## Acceptance Criteria
- Snapshot tests applying each recorded SSE fixture.
