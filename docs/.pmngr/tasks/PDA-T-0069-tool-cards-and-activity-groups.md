---
id: PDA-T-0069
type: task
title: Tool cards and activity groups
status: backlog
parent: PDA-US-0019
milestone: PDA-M-0004
author: mcp
labels: [chat]
estimate: 3
created: 2026-10-11T21:51:26Z
updated: 2026-10-11T22:02:19Z
---

## Description
Card per `id` (kind, title, status pending/in_progress/completed/failed, locations, duration), collapsible groups with a summary ("Ejecutó 3 comandos · leyó 2 ficheros"), generic JSON in/out.

## Acceptance Criteria
- `tool_call_update` does not duplicate cards.
