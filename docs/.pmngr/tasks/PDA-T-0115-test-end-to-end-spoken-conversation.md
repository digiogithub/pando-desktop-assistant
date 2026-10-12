---
id: PDA-T-0115
type: task
title: Test end-to-end spoken conversation
status: in_review
parent: PDA-US-0033
milestone: PDA-M-0007
assignees: [claude-sonnet]
author: mcp
labels: [voice-out, testing]
estimate: 2
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T08:47:05Z
started: 2026-10-12T08:47:05Z
---

## Description
Test `#[ignore]` and manual script: WAV injected as mic → real Pando → TTS to file sink; measures latency budgets (`docs/02` table) and verifies that tool output is not spoken.

## Acceptance Criteria
- Per-stage latency report within budget.
