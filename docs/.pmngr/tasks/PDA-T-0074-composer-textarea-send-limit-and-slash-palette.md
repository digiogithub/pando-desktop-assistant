---
id: PDA-T-0074
type: task
title: "Composer: textarea, send, limit and slash palette"
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
GPUI Kit `TextareaState`; Enter sends, Shift+Enter adds a new line; 8,000-character counter; `/` palette from `GET /api/v1/commands` (name, description, accepts args) with filtering; ↑/↓ prompt history.

## Acceptance Criteria
- IME (es, ja) works; commands are sent as prompt text.
