---
created_at: 2026-10-12T07:41:39.921384137Z
updated_at: 2026-10-12T07:41:39.921384137Z
tags:
    - feature
    - organism
    - PDA-US-0011
---
# Organism view in the app (PDA-US-0011)

Part of [[pando-assistant/plans/backlog-execution_plan.md]]. Builds on [[pando-assistant/changes/organism-simulation-crate.md]], [[pando-assistant/features/app-shell-us-0002.md]] and the paths spike docs/spikes/T-0024-gpui-paths-performance.md.

## What changed
- `OrganismView` (GPUI entity) paints the organism `DrawList` with GPUI paths, text last (PDA-T-0038).
- Pointer, hover, click, keyboard (actions), focus ring and AccessKit roles/labels per cell (PDA-T-0039).
- `pando-assistant --demo`: `DemoView` with scripted fake agents, state selector (keys 1-5), legend, timings overlay (PDA-T-0040).
- tiny-skia raster backend (feature `raster`, default) used when GPUI reports a software GPU (PDA-T-0042).
- Frame-time overlay and measurements (PDA-T-0043); SSIM check against Chrome (PDA-T-0041).

## Files and symbols
- `crates/app/src/organism_view/`: `mod.rs` (OrganismView, OrganismEvent, RenderMode, bind_keys, aria_label), `core.rs` (Core, FrameStats), `paint.rs` (paint_paths, paint_text, TextCache, focus ring, timings), `geometry.rs` (dash_polyline), `raster.rs`, `adapter.rs` (TerminalReaper: remove Done/Error cells after 10 s), `demo.rs`, `demo_source.rs`, `visual.rs`.
- `crates/app/src/ui/paint.rs` (facade: PathBuilder/lyon stroke options, raster_image::Frame).
- Locale files `assets/locales/organism.{es,en}.yml`.
- `crates/organism/harness/chrome_capture.mjs`, `script/organism-visual.sh`.
- Small edits to shared files: `app.rs` (demo window, bind_keys), `cli.rs` (--demo), `lib.rs`, `ui/mod.rs` (one `pub mod paint;`), `Cargo.toml`.

## Why
Milestone M1 (organism on screen). Rendering follows ADR-0006 (direct GPUI paths, one pass of paths then text).

## Verification
`cargo test -p app` (57 pass), clippy `-D warnings` with and without `raster`, `cargo xtask check-deps`; xvfb screenshots in GPU and raster mode; xdotool click/Tab/keys; SSIM vs Chrome 0.993-0.998 on six frames; release CPU 0.54 ms/frame (GPUI) at 6 cells. Not verified: real compositor frame rate, hidden-window frame count, screen reader, design sign-off.
