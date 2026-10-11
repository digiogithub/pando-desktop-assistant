---
id: PDA-T-0011
type: task
title: Token generators for motion, organism, engines and status
status: backlog
parent: PDA-US-0003
milestone: PDA-M-0001
author: mcp
labels: [design, organism]
estimate: 2
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:02:09Z
---

## Description
Generate constants for `motion` (springs, easing `cubic-bezier(.34,1.9,.5,1)`), `engines` (Álamo, Claude blue and Marfil colors) and `status`; `organism` is loaded at runtime with serde (PDA-SP-0004.R2) but its schema is validated in xtask.

## Acceptance Criteria
- An unknown or missing key in `organism` makes `tokens --check` fail.
