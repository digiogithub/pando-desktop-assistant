---
id: PDA-US-0002
type: story
title: As a user I want to open a native window with the Pando visual identity
status: backlog
priority: critical
parent: PDA-EP-0001
milestone: PDA-M-0001
author: mcp
labels: [infra, shell]
created: 2026-10-11T21:46:17Z
updated: 2026-10-11T22:02:01Z
---

## Description
Minimal GPUI app on `gpui-kit =0.7.1`: `ui/` facade, tokio bridge, event pump, embedded fonts, frameless window with its own title bar, platform paths, logging, atomic settings, i18n.

## Acceptance Criteria
- `cargo run -p app` opens a Bosque window with the wordmark (SVG from `design-system/brand/`) in Space Grotesk.
- Test "only `ui/` names `gpui_kit`" passes.
- `--smoke-test` exits after 2 frames (for CI).

## Notes
Implements the base of PDA-SP-0009.R1, R5, R6. Refs: `docs/research/bitacora-gpui.md` §1.3, §1.6, §3.1–3.6.
