---
id: PDA-T-0029
type: task
title: Params from tokens.json + Extras and public types
status: in_progress
parent: PDA-US-0009
milestone: PDA-M-0002
assignees: [claude-sonnet]
author: mcp
labels: [organism]
estimate: 2
created: 2026-10-11T21:48:43Z
updated: 2026-10-11T22:27:06Z
started: 2026-10-11T22:27:06Z
---

## Description
`params.rs` (serde of `tokens.json` via `include_str!`, colors → `Rgba`), `Extras` with hidden constants (noise phases `[1, 2.1, 0.7]`, organelle speed range 0.2–0.8, unscaled px constants §1.18), `types.rs` (`BodyState`, `Engine`, `Status`, `CellId`, `CellInput{is_new, dim, label}`, `Inputs{state, mic_rms, tts_rms, pointer, selected, cells, reduced_motion}`).

## Acceptance Criteria
- Test loads every key used in `docs/research/organism-port.md` §1.
