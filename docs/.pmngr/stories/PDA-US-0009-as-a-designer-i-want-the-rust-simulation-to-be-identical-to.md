---
id: PDA-US-0009
type: story
title: As a designer I want the Rust simulation to be identical to the organism.html reference
status: backlog
priority: high
parent: PDA-EP-0003
milestone: PDA-M-0002
author: mcp
labels: [organism]
created: 2026-10-11T21:47:55Z
updated: 2026-10-11T22:05:25Z
---

## Description
Pure `crates/organism`: parameters from tokens, types, mulberry32 RNG, noise, springs, envelope, `step`, `shape` → `DrawList`, with a JS harness that generates goldens and a `parity` mode.

## Acceptance Criteria
- Implements PDA-SP-0004.R1, R2, R5.
- Parity with goldens at frames 0/1/30/120/600 for the 5 states, 4 engines and 5 statuses.
- `step+shape` < 0.3 ms with 6 cells, < 1 ms with 32.

## Notes
Tasks T1–T6 of `docs/research/organism-port.md` §4.
