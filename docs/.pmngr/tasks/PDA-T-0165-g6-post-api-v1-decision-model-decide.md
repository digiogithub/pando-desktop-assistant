---
id: PDA-T-0165
type: task
title: G6 · POST /api/v1/decision-model/decide
status: backlog
priority: medium
parent: PDA-US-0048
milestone: PDA-M-0006
author: mcp
labels: [upstream-pando]
estimate: 3
created: 2026-10-11T21:57:00Z
updated: 2026-10-11T22:03:16Z
---

## Description
Proxy of `{state, questions}` to the configured provider, reusing timeout, keep-alive, key and the 64 KiB limit; fail-open semantics documented.

## Acceptance Criteria
- The assistant uses TypeSafe/custom with a key without knowing it.
