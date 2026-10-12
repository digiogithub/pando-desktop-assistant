---
id: PDA-T-0106
type: task
title: decisions.jsonl and evaluation CLI
status: in_review
parent: PDA-US-0030
milestone: PDA-M-0006
assignees: [claude-sonnet]
author: mcp
labels: [decide, testing]
estimate: 3
created: 2026-10-11T21:52:55Z
updated: 2026-10-12T08:47:05Z
started: 2026-10-11T22:42:36Z
---

## Description
Append with 10 MB rotation (transcript, state hash, questions, answers, latency, policy, executed); `pando-assistant decisions eval <file> --model <m>` compares agreement and confidence deltas.

## Acceptance Criteria
- Reproducible report over a sample log.
