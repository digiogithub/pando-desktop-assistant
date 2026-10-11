---
id: PDA-T-0030
type: task
title: rng mulberry32, noise, spring and envelope
status: backlog
parent: PDA-US-0009
milestone: PDA-M-0002
author: mcp
labels: [organism]
estimate: 2
created: 2026-10-11T21:48:43Z
updated: 2026-10-11T22:02:37Z
---

## Description
`rng.rs` bit-exact with JS + `hash(id)` for phase; `noise.rs` Σ wᵢ·sin(fᵢ·a + gᵢ·t + k·cᵢ); `spring.rs` semi-implicit Euler from the reference; `envelope.rs` amplitude smoothing (+ synthetic envelope for demo/tests). Internal f64.

## Acceptance Criteria
- Noise periodic in 2π; RNG sequence equals JS for seeds 1..3.
