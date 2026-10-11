---
id: PDA-T-0100
type: task
title: Trust policy engine
status: backlog
parent: PDA-US-0029
milestone: PDA-M-0006
author: mcp
labels: [decide, safety]
estimate: 3
created: 2026-10-11T21:52:55Z
updated: 2026-10-11T22:03:24Z
---

## Description
Pure function `policy(decision, transcript, agents) -> Execute | Confirm(prompt) | Conversation{top2}` with thresholds 0.85/0.6, reversibility per action and a destructive rule (p ≥ 0.95 + literal id). Thresholds are configurable but have a floor.

## Acceptance Criteria
- Exhaustive test table covering the cases in PDA-SP-0008.R2.
