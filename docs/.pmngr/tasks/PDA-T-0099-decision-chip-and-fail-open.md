---
id: PDA-T-0099
type: task
title: Decision chip and fail-open
status: in_review
parent: PDA-US-0028
milestone: PDA-M-0006
assignees: [claude-sonnet]
author: mcp
labels: [decide, shell]
estimate: 2
created: 2026-10-11T21:52:55Z
updated: 2026-10-12T08:47:05Z
started: 2026-10-12T08:47:05Z
---

## Description
`decisión · <model>` chip with health from `/decision-model/router/health` (60 s cache) and refresh via `/config/events`; any failure ⇒ conversation + "decisión: no disponible" chip.

## Acceptance Criteria
- Disabling Ollama keeps the voice conversation working.
