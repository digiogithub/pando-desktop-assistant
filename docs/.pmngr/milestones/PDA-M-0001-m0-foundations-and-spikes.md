---
id: PDA-M-0001
type: milestone
title: M0 · Foundations and spikes
status: backlog
author: mcp
created: 2026-10-11T21:40:29Z
updated: 2026-10-11T22:01:07Z
due: 2026-10-30
---

## Description
Rust workspace ready (toolchain, lints, xtask, CI), GPUI facade, design-system tokens generated, and spikes that unblock risky decisions: Whistle FFI + Spanish WER, Windows sidecar, single sherpa-onnx, audio/AEC, Wayland hotkeys, espeak-ng licence.

## Acceptance Criteria
- `cargo build` + `cargo test` + clippy `-D warnings` green in CI on Linux/macOS/Windows.
- Empty GPUI app opens a window with Pando fonts and tokens.
- Every spike closed with a report in `docs/spikes/` and a recorded decision (ADR).

## Notes
Refs: `docs/01-strategy.md`, `docs/research/bitacora-gpui.md` §6, `docs/research/voice-stack.md` §9.
