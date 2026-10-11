---
id: PDA-US-0018
type: story
title: As a user I want to see the response being typed in real time with clean markdown
status: backlog
priority: critical
parent: PDA-EP-0006
milestone: PDA-M-0004
author: mcp
labels: [chat]
created: 2026-10-11T21:50:29Z
updated: 2026-10-11T22:02:41Z
---

## Description
Transcript with grouped bubbles, streaming markdown (GPUI Kit `TextViewState::markdown` + `push_str`, code highlighting, copy), collapsible thinking, notice rows, autoscroll with a "ir al final" pill.

## Acceptance Criteria
- Implements PDA-SP-0002.R3.
- ≥ 50 fps with 2.000 deltas in 3 s.
