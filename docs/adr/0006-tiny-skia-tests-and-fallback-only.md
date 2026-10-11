# 0006 · Organism renders with GPUI paths; `tiny-skia` only for tests and fallback

- **Status:** accepted
- **Date:** 2026-10-12
- **Deciders:** project owner, planning agents
- **Refs:** PDA-T-0024, PDA-T-0038, PDA-T-0042, [`docs/research/organism-port.md`](../research/organism-port.md) §3, §5

## Context

The organism is ~2k path points per frame. gpui-pre 0.3.8 offers `canvas()` +
`PathBuilder` (lyon tessellation, 4× MSAA) and `paint_image` for raster blits. A
`tiny-skia` raster path is portable and pixel-deterministic, but uploads a full frame per
tick (~6.5 MB at 1280² × 2) and leaks atlas entries if `drop_image` is forgotten.

## Decision

`crates/organism` emits a backend-neutral `DrawList`. The production renderer paints it with
GPUI paths (`window.paint_path`). A `tiny-skia` backend exists behind the `raster` feature
for golden PNG tests, offline exports and as a fallback if the GPUI paths spike
(PDA-T-0024) or a target GPU shows it cannot hold 60 fps; it is not the default runtime path.

## Consequences

- The GPU does the work; idle CPU stays low.
- Golden/visual tests do not need a GPU.
- Two backends must stay in parity; SSIM checks (PDA-T-0041/0042) guard it.

## Alternatives considered

| Option | Why not |
|---|---|
| tiny-skia as the main renderer | Per-frame upload cost, marginal at 4K, atlas churn |
| GPUI paths only, no raster | No GPU-free goldens, no fallback for weak GPUs |
