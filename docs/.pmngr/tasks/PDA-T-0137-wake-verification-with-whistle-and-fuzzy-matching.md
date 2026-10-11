---
id: PDA-T-0137
type: task
title: Wake verification with Whistle and fuzzy matching
status: backlog
parent: PDA-US-0041
milestone: PDA-M-0010
author: mcp
labels: [wake-word]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-11T22:02:45Z
---

## Description
On a VAD segment ≤ 2.5 s: transcribe with bias `Pando\nOye Pando\nHey Pando`, normalize, Levenshtein over the first 1–2 words with a per-user threshold; rest of the phrase → voice orchestrator as a command; `WakeDetector` trait for a future oWW model.

## Acceptance Criteria
- Scenario from PDA-SP-0011.R2.
