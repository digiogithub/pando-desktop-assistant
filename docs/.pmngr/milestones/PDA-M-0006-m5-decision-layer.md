---
id: PDA-M-0006
type: milestone
title: M5 · Decision layer
status: backlog
author: mcp
created: 2026-10-11T21:40:56Z
updated: 2026-10-11T22:02:15Z
due: 2027-01-29
---

## Description
Cascade intent → agent → action → project over `/v1/systemone` (Jev / tev1). Trust policy lives in code; commands are executed against the API, never via the LLM. Fail-open to conversation.

## Acceptance Criteria
- transcript → decision < 100 ms with local `tev1:0.8b` (P50).
- Destructive actions are confirmed by voice unless p ≥ 0.95 and the literal id appears in the transcript.
- Without a decision model: everything is conversation, with a chip "decisión: no disponible".
- `decisions.jsonl` records every decision.

## Notes
Refs: `docs/03-decision-layer.md`, `docs/research/pando-api.md` §5.
