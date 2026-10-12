---
id: PDA-T-0172
type: task
title: "Voice target switch: \"háblale a <agente>\" / \"vuelve\""
status: in_progress
priority: medium
parent: PDA-US-0029
milestone: PDA-M-0006
assignees: [claude-opus]
author: mcp
labels: [decide, shell]
estimate: 3
created: 2026-10-12T14:06:10Z
updated: 2026-10-12T14:06:14Z
started: 2026-10-12T14:06:14Z
---

## Description
Implement PDA-SP-0008.R8 (product-owner decision 2026-10-12): voice commands "háblale a <agente/célula>" and "vuelve" (back to the primary session). In `decide`: intent classes, a cascade step that asks only the agent question for "háblale a", and a policy outcome `SwitchTarget` (non-destructive, executes at p ≥ reversible threshold without confirmation; lower → clarify with top-2). In `app`: wire the outcome to `OrganismPane` target (same as the "Hablarle" button / Esc), spoken feedback via locale files.

## Acceptance Criteria
- Phrasing variants: "háblale a auth", "habla con el de docs", "vuelve", "vuelve a Pando", "volver".
- Unit tests (cascade, policy table), policy property tests (never switches to unknown/all target, never below threshold), app tests (outcome → pane target).
- Live intent eval (tev1:0.8b) does not regress below the 96/100 baseline.

## Notes
Refs: PDA-SP-0008.R8, ADR 0009.
