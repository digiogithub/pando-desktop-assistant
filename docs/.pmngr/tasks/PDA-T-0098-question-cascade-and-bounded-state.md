---
id: PDA-T-0098
type: task
title: Question cascade and bounded state
status: done
parent: PDA-US-0028
milestone: PDA-M-0006
assignees: [claude-opus]
author: mcp
labels: [decide]
estimate: 5
created: 2026-10-11T21:52:55Z
updated: 2026-10-11T23:03:25Z
started: 2026-10-11T22:42:36Z
closed: 2026-10-11T23:03:25Z
---

## Description
Builder for `state` (transcript + agents + projects, budget per model context and 64 KiB); questions `intent`, `agent`, `action`, `engine`, `project` with ordered criteria and descriptions; cascade that only asks what is necessary; result `Decision{intent, agent?, action?, engine?, project?, probs}`.

## Acceptance Criteria
- A set of 100 labeled phrases with > 90 % intent accuracy using `tev1:0.8b` (`#[ignore]` test).
