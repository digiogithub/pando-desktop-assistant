---
id: PDA-T-0064
type: task
title: "Session sidebar: pagination, search, rename and delete"
status: backlog
parent: PDA-US-0017
milestone: PDA-M-0004
author: mcp
labels: [chat]
estimate: 3
created: 2026-10-11T21:51:26Z
updated: 2026-10-11T22:02:19Z
---

## Description
Virtual list (100/page, "cargar más"), running dot, message count, `prompt_preview`, local search, inline rename (`PATCH`), delete with confirmation (`DELETE`), lazy "Nueva".

## Acceptance Criteria
- 1,000 sessions without jank; a new session appears at the top after the first `session` event.
