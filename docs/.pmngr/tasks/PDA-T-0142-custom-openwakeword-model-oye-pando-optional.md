---
id: PDA-T-0142
type: task
title: Custom openWakeWord model "Oye Pando" (optional)
status: in_review
parent: PDA-US-0042
milestone: PDA-M-0010
assignees: [claude-sonnet]
author: mcp
labels: [wake-word]
estimate: 5
created: 2026-10-11T21:56:03Z
updated: 2026-10-12T08:13:09Z
started: 2026-10-12T08:12:36Z
---

## Description
Only if the evaluation does not meet targets: train a model with the livekit-wakeword toolkit (Python only at development time), inference with `oww-rs` (tract) or `livekit-wakeword` inside the worker, behind `WakeDetector`.

## Acceptance Criteria
- Improves FAR/FRR vs VAD+Whistle on the same dataset.
