---
id: PDA-EP-0006
type: epic
title: Native chat
status: backlog
priority: critical
milestone: PDA-M-0004
author: mcp
labels: [chat]
created: 2026-10-11T21:45:36Z
updated: 2026-10-11T22:02:43Z
---

## Description
Full chat UI in GPUI: session list, transcript with streaming markdown (GPUI Kit `TextViewState::markdown` + `push_str`, `MessageScroller` in tail mode), thinking, tool cards with diffs, composer with commands/model/@mentions, steering, cancel, plan/todos/goal/usage, `run_seq` reattach.

## Acceptance Criteria
- PDA-SP-0002.R1–R8 verified.
- Checklist §7.2–7.7 in `docs/research/pando-api.md` complete.

## Notes
Spec: PDA-SP-0002. Patterns: batched event pump and `ChatModel::apply` from Bitácora (`docs/research/bitacora-gpui.md` §2.3, §2.5).
