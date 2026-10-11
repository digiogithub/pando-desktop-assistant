---
id: PDA-T-0010
type: task
title: "xtask tokens: palette, typography and GPUI Kit theme"
status: done
parent: PDA-US-0003
milestone: PDA-M-0001
assignees: [claude-sonnet]
author: mcp
labels: [design, infra]
estimate: 3
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:24:11Z
started: 2026-10-11T22:23:48Z
closed: 2026-10-11T22:24:11Z
---

## Description
Port `xtask/src/tokens.rs`: read `design-system/tokens.json`, generate `crates/app/src/ui/theme/{palette,scale}.rs` and `assets/themes/pando.json` (dark Bosque kit theme). Marfil alphas (62/45/22/12/7 %) as constants.

## Acceptance Criteria
- `cargo xtask tokens --check` is clean after generation; `--contrast` reports pairs below 4.5:1.
