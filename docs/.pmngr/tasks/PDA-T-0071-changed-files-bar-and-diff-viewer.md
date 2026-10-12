---
id: PDA-T-0071
type: task
title: Changed files bar and diff viewer
status: done
parent: PDA-US-0019
milestone: PDA-M-0004
assignees: [claude-sonnet]
author: mcp
labels: [chat]
estimate: 3
created: 2026-10-11T21:51:26Z
updated: 2026-10-12T10:57:56Z
started: 2026-10-12T08:06:42Z
closed: 2026-10-12T10:57:56Z
---

## Description
Aggregate changes from `tool_result.diff`, `raw_output.metadata.files_changed` and `agentvcs/sessions/{id}/diff` (+ blobs) in a bar with +/- per file; native side-by-side/unified diff viewer.

## Acceptance Criteria
- Counters match the WebUI for the same session.
