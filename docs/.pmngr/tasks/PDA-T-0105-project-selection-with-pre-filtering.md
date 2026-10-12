---
id: PDA-T-0105
type: task
title: Project selection with pre-filtering
status: in_review
parent: PDA-US-0030
milestone: PDA-M-0006
assignees: [claude-sonnet]
author: mcp
labels: [decide]
estimate: 3
created: 2026-10-11T21:52:55Z
updated: 2026-10-12T08:47:05Z
started: 2026-10-11T22:42:36Z
---

## Description
`project` question over the names + `current`; if there are more than 12 projects, pre-filter by fuzzy similarity (and Whistle/`needle_embed` embeddings or the Pando KB if it provides them); update the pill with confidence; used in spawn and "cambia a …".

## Acceptance Criteria
- 40 synthetic projects: the correct one is in the top 12 in ≥ 98 % of test cases.
