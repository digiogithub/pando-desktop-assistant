---
created_at: 2026-10-11T22:23:59.994563203Z
updated_at: 2026-10-11T22:23:59.994563203Z
tags:
    - change
    - tokens
---
# xtask tokens generator (PDA-T-0010, PDA-T-0011)

Builds on [[pando-assistant/plans/backlog-execution_plan.md]].

## What changed
- New `cargo xtask tokens [--check|--contrast]`. Reads `design-system/tokens.json` and writes:
  - `crates/app/src/ui/theme/palette.rs` (colours as `u32`, Marfil alpha ramp as `Rgba`),
  - `crates/app/src/ui/theme/scale.rs` (fonts, text scale, space, radius, stroke),
  - `crates/app/assets/themes/pando.json` (dark Bosque GPUI Kit theme, "Pando Dark"),
  - `crates/organism/src/tokens.rs` (Color, motion, `ENGINES`, `STATUSES`, `ORGANISM_*` consts).
- `--check` fails if any output is stale or if the organism-side schema is invalid
  (unknown or missing key in `motion`, `organism`, `engines`, `status`).
- `--contrast` prints WCAG ratios; Bermellon is checked at 3:1 (graphic, not text, 4.1:1 actual).

## Files and symbols
`xtask/src/tokens.rs` (generators, `check`, `contrast_rows`, `rustfmt`), `xtask/src/tokens_organism.rs`
(`validate_all`, `generate`), `xtask/src/main.rs`, `crates/organism/src/lib.rs` (`pub mod tokens`),
`crates/app/src/ui/mod.rs` (`pub mod theme;`), `crates/app/src/ui/theme/mod.rs`.

## Why
PDA-T-0010 / PDA-T-0011 (story PDA-US-0003). Pattern ported from Bitacora's xtask tokens.

## Verification
`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p xtask -p organism`
(stale-output test, rustfmt-stability test, schema negative tests, contrast test) all pass;
`cargo xtask tokens --check` clean.
