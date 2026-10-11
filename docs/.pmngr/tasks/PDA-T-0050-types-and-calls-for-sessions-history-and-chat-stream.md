---
id: PDA-T-0050
type: task
title: Types and calls for sessions, history and chat stream
status: in_review
parent: PDA-US-0013
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [pando-client, chat]
estimate: 5
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:34:26Z
started: 2026-10-11T22:34:21Z
---

## Description
`Session`/`Message` in PascalCase; untagged `Part` discriminated by keys (text, thinking, tool call, tool result, finish, image url, binary, fallback `Other(Value)`); `ChatEvent` enum with all events from §3.3; calls: `list_sessions`, `get_session`, `rename`, `delete`, `chat_stream`, `attach_stream`, `steer`, `cancel`, `pending`, `auto_approve`, `session_model`, `goal`, `goal_cancel`, `permissions_respond`, `questions_respond`.

## Acceptance Criteria
- All chat fixtures parse; fold test tool_result → tool_call.
