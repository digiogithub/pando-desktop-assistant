---
id: PDA-T-0065
type: task
title: Load history and map to transcript
status: backlog
parent: PDA-US-0017
milestone: PDA-M-0004
author: mcp
labels: [chat]
estimate: 2
created: 2026-10-11T21:51:26Z
updated: 2026-10-11T22:02:19Z
---

## Description
`GET /sessions/{id}` → ChatModel: folding results into tool calls, unknown parts as JSON blocks, images, finish reasons (cancelled, max_tokens, permission denied) as notices; `[delegated-result]` blocks as conclusion rows.

## Acceptance Criteria
- A real Pando session with tools looks the same as in the WebUI.
