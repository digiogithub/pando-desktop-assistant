---
id: PDA-T-0108
type: task
title: Sentence segmenter and synthesis in pipeline
status: done
parent: PDA-US-0031
milestone: PDA-M-0007
assignees: [claude-sonnet]
author: mcp
labels: [voice-out]
estimate: 3
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T07:55:39Z
started: 2026-10-12T07:54:44Z
closed: 2026-10-12T07:55:39Z
---

## Description
Split on `[.!?;:]` and line breaks, respecting abbreviations/numbers/decimals; first chunk cut at a comma if it has more than 12 words; synthesize n+1 while n is playing; clean cancellation by id.

## Acceptance Criteria
- No audible gaps between sentences; segmenter tests with 50 cases.
