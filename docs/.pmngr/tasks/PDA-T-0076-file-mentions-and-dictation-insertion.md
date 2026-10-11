---
id: PDA-T-0076
type: task
title: "@file mentions and dictation insertion"
status: backlog
parent: PDA-US-0021
milestone: PDA-M-0004
author: mcp
labels: [chat]
estimate: 2
created: 2026-10-11T21:51:27Z
updated: 2026-10-11T22:02:19Z
---

## Description
`@` opens a search via `/api/v1/files/search` of the target project; API `Composer::insert_dictation(text)` used by the decision layer (visual "dictado" mark).

## Acceptance Criteria
- A mention inserts the relative path; dictation does not send.
