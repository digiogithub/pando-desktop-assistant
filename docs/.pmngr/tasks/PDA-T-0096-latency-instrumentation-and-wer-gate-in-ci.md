---
id: PDA-T-0096
type: task
title: Latency instrumentation and WER gate in CI
status: in_review
parent: PDA-US-0027
milestone: PDA-M-0005
assignees: [claude-sonnet]
author: mcp
labels: [voice-in, testing]
estimate: 3
created: 2026-10-11T21:52:55Z
updated: 2026-10-12T08:18:02Z
started: 2026-10-12T08:05:39Z
---

## Description
`tracing` traces with timings per stage (release → PCM ready → transcript → decision → API); debug panel; small set of Spanish WAV files in the repo and a test that fails if WER worsens by more than 2 points.

## Acceptance Criteria
- P50/P95 per stage visible in the debug panel and in the logs.
