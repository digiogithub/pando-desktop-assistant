---
created_at: 2026-10-11T22:21:10.043563439Z
updated_at: 2026-10-11T22:21:10.043563439Z
tags:
    - pando-assistant
    - change
    - docs
    - adr
    - design
---
# Docs: ADR log, planning docs reconciled with research, organism design decisions

Story PDA-US-0005; tasks PDA-T-0015, PDA-T-0016, PDA-T-0017. Part of [[pando-assistant/plans/backlog-execution_plan.md]].

## What changed
- **PDA-T-0015 (commit 327b5d2)**: new `docs/adr/` with `0000-template.md`, `README.md` index and ADRs 0001 gpui-kit =0.7.1 pin + `ui/` facade, 0002 REST + `chat/stream` SSE (not AG-UI), 0003 rust-i18n, 0004 separate `pando-voice` worker process, 0005 sherpa-onnx as the only ONNX Runtime, 0006 GPUI paths for the organism with tiny-skia only for tests/fallback. Linked from `docs/README.md`; AGENTS.md §6 got one ADR rule.
- **PDA-T-0016 (commit 3853683)**: `docs/01`–`06`, `docs/README.md` and `design-system/README.md` §6 corrected with [[pando-assistant research pando-api]] / voice-stack / bitacora-gpui findings: ephemeral `X-Pando-Token` from `/api/v1/token`, no `POST /sessions`, real SSE events (`todos_update`, `done`, no `response`), notifications are toasts only, projected `TaskResponse`, no HTTP pause, conclusion vocabulary `success|partial|failed|blocked`, `[delegated-result]` flow, needle C API (bytes, JSON out, libc++, no depth, streaming), rubato 5, global-hotkey X11 only + ashpd, `Entity<Organism>` + `window.request_animation_frame()`. No `[verify]` left; unknowns linked to spikes or upstream gap tasks PDA-T-0155..0170.
- **PDA-T-0017 (commit 0ffd1e2)**: `design-system/README.md` §7 decision table; `tokens.json` updated (breath 4.8 s via `breathRadPerSecond`, `cellBeat.hz` renamed `radPerSecond`, new `cellBlink`, `errorTremor`, `nucleusScale`, `pendingAlpha`, `ring` (stable slots, 12-cell cap, +N overflow), `scaling` floors, `waves.emitOn`, cellBirth 1.6 s); `reference/organism.html` follows the rename and treats error as dead in the bulge.

## Open for the product owner
Claude blue / Bermellón sign-off, 12-cell cap + "+N" overflow, dropping idle/listening ripples (defaults applied).

## Verification
Docs-only. `grep '[verify'` on docs/0*.md empty; tokens.json parses; every token path read by organism.html exists in tokens.json (43 paths); `node --check` on the reference script passes. Browser visual check not possible (headless browser unavailable).
