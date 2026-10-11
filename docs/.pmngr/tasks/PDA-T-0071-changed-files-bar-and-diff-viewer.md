---
id: PDA-T-0071
type: task
title: Changed files bar and diff viewer
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
Aggregate changes from `tool_result.diff`, `raw_output.metadata.files_changed` and `agentvcs/sessions/{id}/diff` (+ blobs) in a bar with +/- per file; native side-by-side/unified diff viewer.

## Acceptance Criteria
- Counters match the WebUI for the same session.
