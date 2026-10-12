---
created_at: 2026-10-11T22:43:14.543413347Z
updated_at: 2026-10-11T22:43:14.543413347Z
tags:
    - change
    - organism
    - PDA-US-0009
    - PDA-US-0010
---
# crates/organism: simulation, draw list, lifecycle, reduced motion, semantics

Builds on [[pando-assistant/plans/backlog-execution_plan.md]] and [[docs/research/organism-port.md]]. Serves PDA-T-0029 .. PDA-T-0037 (stories PDA-US-0009, PDA-US-0010; spec PDA-SP-0004 R1-R5, R7, R8).

## What changed
The `organism` crate went from a tokens-only skeleton to the full pure simulation:
- `params.rs` `Params` (every number from the generated `tokens.rs`, widened f32 -> f64 through the decimal string so `0.13f32` becomes `0.13f64`) and `Extras` (hidden reference constants: 6.283, 6.28, organelle speed range, hover 34 / click 40, px widths, alphas...). `types.rs`: `BodyState`, `Engine`, `Status` (adds `Pending`), `CellId(Arc<str>)`, `CellInput`, `Inputs` (pointer `[f64;2]` normalised, `mic_rms`/`tts_rms` are `Option`, `None` = synthetic envelope).
- `rng.rs` mulberry32 bit-exact with JS plus FNV `hash_unit`; `noise.rs`; `spring.rs` (reference Euler + monotone critical variant); `envelope.rs`.
- `sim.rs` `Organism::step` (amplitude, waves, cells, organelles, nucleus tracking moved from draw with `1-(1-0.08)^(60 dt)`), `Config { parity, seed }`. Parity mode reproduces `organism.html` exactly; the default mode applies the design decisions.
- `body.rs` `body_radius` with per-step cached bulge terms; `shape.rs` `Organism::shape/shape_into` -> `DrawList` (`Circle`, `Poly`, `Curve`, `Discs`, `Text`, `ClipBegin/End`) in reference draw order; `dash.rs` crate-side dashing with phase (`dash_polyline`, `resolve_dashes`, `flatten_curve`).
- `lifecycle.rs` stable ring slots (lowest free slot, compaction after 10 quiet seconds), death (0.6 s fade + pseudopod retract + bulge fade, revivable), birth wave (gated 0.2 s against Delegating), `Organism::delegate()` 1.5 s overlay (`state.rs` `BodyMachine`, documented transition table), `+N` fold above 12 cells (only done/paused cells, id `__fold__`).
- `layout.rs` `Scene::layout(size, scale_factor)` (scale W/640, 0.75 device px stroke floor, 2 px halo floor, LOD 160/96, 48/32, 70/30, labels and badges hidden under 200 px). Reduced motion: no breathing / noise / ripple / wobble / drift, monotone springs, static organelles, dashes and satellites, waves fade in place, steady blink and no beat/tremor.
- `semantics.rs` `Organism::semantics()` (bounds, ids, engine/status ids, selection, aggregate + folded) and `hit_test`.

## Files
`crates/organism/src/{params,types,rng,noise,spring,envelope,state,sim,lifecycle,body,shape,dash,draw,layout,semantics}.rs`; tests in `crates/organism/tests/{params,golden,properties,lifecycle,reduced_motion,semantics}.rs`; harness `crates/organism/harness/harness.mjs` with golden `crates/organism/tests/golden/organism.json`; bench `crates/organism/benches/step_shape.rs`.

## Why / deviations from the plan
- Params come from the generated Rust consts, not serde of tokens.json at runtime (no serde in the pure crate; the test compares against tokens.json).
- Harness lives under `crates/organism/harness/` (scope), not `design-system/reference/`. `node crates/organism/harness/harness.mjs --check` verifies the committed golden.
- Parity differences deliberately not replicated: round caps never reset, `lineDashOffset` persisting into later dashed strokes (only the nucleus orbit carries a dash phase).
- Delegating overlay does not change the radius table; it adds a wave and a 3 % sine bulge pulse. Terminal removal after 10 s (`removeTerminalAfter`) stays the adapter's job (it controls the list).

## Verification
`cargo fmt --all`, `cargo clippy -p organism --all-targets -- -D warnings`, `cargo test -p organism` (unit, golden, 10k-case proptest, lifecycle, reduced motion, semantics), `cargo xtask check-deps` all green. Golden parity: state within 1e-8 (task asked 1e-6) at frames 0/1/30/120/210/300/450/600 covering all 5 states, 4 engines, 5 statuses, spawn, kill, pointer hover; draw ops within 1e-3 px, alpha 1e-4. Bench (Ultra 9 285, release): step+shape 63 us (6 cells), 276 us (32 cells). `gintrack verify_requirement` could not stamp: no test results ingested.
