---
id: PDA-EP-0003
type: epic
title: Living organism (simulation + GPUI rendering)
status: backlog
priority: high
milestone: PDA-M-0002
author: mcp
labels: [organism]
created: 2026-10-11T21:45:36Z
updated: 2026-10-11T22:02:27Z
---

## Description
Full port of `organism.html` to `crates/organism` + GPUI `OrganismView`, with goldens harness, visual parity, cell lifecycle, reduced motion, LOD and raster fallback.

## Acceptance Criteria
- Requirements PDA-SP-0004.R1–R8 verified.
- Demo with fake agents and a keyboard state selector.

## Notes
Spec: PDA-SP-0004. Detailed plan: `docs/research/organism-port.md` §2–§4.
