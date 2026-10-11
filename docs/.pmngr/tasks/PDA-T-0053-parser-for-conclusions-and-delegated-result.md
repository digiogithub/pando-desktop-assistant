---
id: PDA-T-0053
type: task
title: Parser for conclusions and [delegated-result]
status: done
parent: PDA-US-0013
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [pando-client]
estimate: 2
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:35:51Z
started: 2026-10-11T22:35:42Z
closed: 2026-10-11T22:35:51Z
---

## Description
Tolerant parser for `<pando:conclusion>` (YAML, last block wins, `status: success|partial|failed|blocked`, normalized `done`) and for the `[delegated-result task=… engine=… project=… status=… confidence=…]` header + `summary:/follow_up:/artifacts:` in history.

## Acceptance Criteria
- Cases from `internal/mesnada/conclusion/parse.go` ported as tests.
