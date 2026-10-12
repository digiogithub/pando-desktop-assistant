---
id: PDA-T-0110
type: task
title: Text normalizer for speech (es/en)
status: done
parent: PDA-US-0032
milestone: PDA-M-0007
assignees: [claude-sonnet]
author: mcp
labels: [voice-out]
estimate: 3
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T07:55:39Z
started: 2026-10-12T07:54:44Z
closed: 2026-10-12T07:55:39Z
---

## Description
Strip markdown; fences → "bloque de código omitido"; inline code and humanized ids (`migracion-auth` → "migración auth", `handlers_chat.go` → "handlers chat punto go"); numbers/units/percentages/dates to words; URLs → host; emoji removed; English equivalent.

## Acceptance Criteria
- 100 test cases, including those of PDA-SP-0007.R3.
