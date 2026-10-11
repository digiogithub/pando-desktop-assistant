---
id: PDA-T-0070
type: task
title: Renderers per tool type
status: backlog
parent: PDA-US-0019
milestone: PDA-M-0004
author: mcp
labels: [chat]
estimate: 5
created: 2026-10-11T21:51:26Z
updated: 2026-10-11T22:02:19Z
---

## Description
execute (`$ cmd`, truncatable monospace output, exit code), edit (colored unified diff from `diff{old_string,new_string}`), write (content), read (truncated to 2,000 chars), search/grep/glob (summary + results), fetch (URL + summary), images (`data:` thumbnails), JSON fallback.

## Acceptance Criteria
- Snapshot per renderer with real fixtures.
