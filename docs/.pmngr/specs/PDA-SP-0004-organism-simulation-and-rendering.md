---
id: PDA-SP-0004
type: spec
title: "Organism: simulation and rendering"
status: backlog
author: mcp
labels: [organism]
created: 2026-10-11T21:41:32Z
updated: 2026-10-11T22:42:54Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/organism/src/sim.rs#Organism::step
        - crates/organism/src/rng.rs#Mulberry32
      tests:
        - crates/organism/tests/golden.rs#simulation_state_matches_reference_at_recorded_frames
        - crates/organism/tests/properties.rs#no_nan_and_invariants_hold
        - crates/organism/tests/properties.rs#no_energy_blow_up_at_the_largest_dt
  R2:
    status: backlog
    trace:
      code:
        - crates/organism/src/params.rs#Params
        - crates/organism/src/params.rs#Extras
      tests: [crates/organism/tests/params.rs#params_match_tokens_json_exactly]
  R3:
    status: backlog
    trace:
      code:
        - crates/organism/src/state.rs#BodyMachine
        - crates/organism/src/envelope.rs#Amplitude
      tests:
        - crates/organism/src/state.rs#documented_transitions_are_listed
        - crates/organism/tests/lifecycle.rs#delegating_overlay_lasts_one_and_a_half_seconds_and_returns_to_the_previous_state
  R4:
    status: backlog
    trace:
      code: [crates/organism/src/lifecycle.rs#Organism::reconcile]
      tests:
        - crates/organism/tests/lifecycle.rs#dying_cell_fades_retracts_and_is_removed_after_the_death_duration
        - crates/organism/tests/lifecycle.rs#birth_emits_its_own_wave_and_starts_at_the_centre
        - crates/organism/tests/lifecycle.rs#adding_or_removing_a_cell_does_not_move_the_others_slots
  R5:
    status: backlog
    trace:
      code: [crates/organism/src/shape.rs#Organism::shape]
      tests:
        - crates/organism/tests/golden.rs#draw_list_matches_reference_draw_calls
  R6:
    status: backlog
  R7:
    status: backlog
    trace:
      code:
        - crates/organism/src/semantics.rs#Organism::semantics
        - crates/organism/src/semantics.rs#Organism::hit_test
      tests:
        - crates/organism/tests/semantics.rs#semantics_match_drawn_positions_within_one_pixel
        - crates/organism/tests/semantics.rs#hit_test_picks_the_nearest_cell_inside_the_hit_radius
  R8:
    status: backlog
    trace:
      code:
        - crates/organism/src/layout.rs#Scene
        - crates/organism/src/sim.rs#Organism::step
      tests:
        - crates/organism/tests/reduced_motion.rs#small_canvas_drops_labels_and_keeps_strokes_legible
        - crates/organism/tests/reduced_motion.rs#springs_are_monotone_and_do_not_overshoot
        - crates/organism/tests/reduced_motion.rs#breathing_noise_and_ripple_are_zero
---

## Purpose
The orb as a "living organism": mesnada as the mother cell, each agent as a daughter cell; body states driven by voice. Pure, deterministic simulation plus native GPUI rendering.

## Scope
`crates/organism` (no GPUI): `step(dt, Inputs)`, `shape(Scene) -> DrawList`, tokens from `design-system/tokens.json`. GPUI adapter (`canvas` + `PathBuilder`), `tiny-skia` raster fallback, mini scale 118 px, reduced motion.

Refs: `design-system/README.md`, `design-system/reference/organism.html`, `docs/research/organism-port.md`.

## Requirements

### PDA-SP-0004.R1 — Pure and deterministic simulation

`crates/organism` SHALL have no GPUI, network or audio dependency and expose `Organism::step(dt, &Inputs)` and `Organism::shape(&Scene) -> DrawList`. It SHALL simulate internally in a 640-unit reference canvas in f64, clamp `dt` to 0.05 s, use a seeded RNG (mulberry32, bit-identical to the JS harness) and derive each cell's phase from a hash of its id. A `parity` mode SHALL reproduce the reference exactly.

#### Scenario: golden parity
- GIVEN the JS harness timeline and seed
- WHEN Rust replays it in parity mode
- THEN draw lists match at frames 0/1/30/120/600 (points ±1e-3 px, alpha ±1e-4)

### PDA-SP-0004.R2 — Parameters from tokens.json

All visual constants SHALL come from `design-system/tokens.json` (embedded with `include_str!`), plus an explicit `Extras` struct for constants the reference hides (noise phases `[1, 2.1, 0.7]`, organelle speed range, unscaled pixel constants). Changing a token SHALL require no code change.

#### Scenario: token change
- WHEN `organism.ringRadius` changes in tokens.json
- THEN the next build places cells on the new ring with no Rust edits

### PDA-SP-0004.R3 — Voice-driven body states

The body SHALL implement states Idle, Listening, Thinking, Speaking and a transient Delegating overlay (1.5 s, returns to the previous state), with the per-state membrane radius, nucleus scale, organelle/vesicle speed, dashed orbit + satellites (thinking) and waves (speaking) of the reference. In Listening/Speaking the amplitude SHALL be the real `mic_rms`/`tts_rms` (0..1, smoothed with `amplitudeLerpPerSecond`) instead of the synthetic envelope.

#### Scenario: speaking with silence
- GIVEN Speaking and `tts_rms = 0`
- THEN the membrane ripple and nucleus pulse decay to rest levels

### PDA-SP-0004.R4 — Cell lifecycle: mitosis and death

A new cell SHALL bud from the body centre (bulge +1.6 decaying over 2.2 s, radius growth over the birth duration, soft spring k 2.2) and emit one wave itself. A removed cell SHALL fade over 0.6 s while its pseudopod retracts, then be deleted; a cell that reappears while dying SHALL revive. Cells SHALL keep stable ring slots (no reflow jump when others are added or removed).

#### Scenario: kill one of five
- WHEN cell 3 disappears from inputs
- THEN it fades and retracts in 0.6 s and cells 1,2,4,5 keep their angles

### PDA-SP-0004.R5 — Engine and status encoding

Cells SHALL encode engine by nucleus fill/colour (Pando solid Álamo, Claude ACP solid Claude blue, Claude external hollow Claude blue, CLI hollow Marfil + dashed "spore" membrane) and status by motion/opacity plus badge (working beat, waiting blink + inward Marfil vesicles + `?`, paused 35 % + `II`, done Marfil 45 % + `✓`, error Bermellón tremor + `!`). Status SHALL never be colour alone.

#### Scenario: colour-blind check
- GIVEN a greyscale render
- THEN every status is still distinguishable by badge or motion

### PDA-SP-0004.R6 — GPUI rendering at 60 fps

The GPUI adapter SHALL paint the draw list with `canvas()` + `PathBuilder`/`window.paint_path`, emitting all geometry as paths in reference draw order and all text afterwards, with crate-side dashing (phase support) and analytic organelle clipping. It SHALL sustain 60 fps at 640 px with 6 and 32 cells, ≤ 4 ms CPU per frame for 6 cells, and request no frames while the window is hidden (resetting `dt` on show).

#### Scenario: minimise
- WHEN the window is minimised for 10 s
- THEN 0 frames are rendered and the first visible frame uses dt ≤ 0.05

### PDA-SP-0004.R7 — Interaction: hover, click, keyboard

The nucleus SHALL follow the pointer (frame-rate independent lerp). Hovering within 34 px (scaled) SHALL scale the cell 1.25×; clicking within 40 px SHALL select the nearest cell (selection ring). Cells SHALL be reachable by keyboard (Tab/arrow cycling, Enter opens card) and exposed with accessible role, label and status.

#### Scenario: keyboard only
- WHEN the user tabs into the canvas and presses → twice and Enter
- THEN the third cell's card opens

### PDA-SP-0004.R8 — Scaling, LOD and reduced motion

`Scene::layout(size, scale_factor)` SHALL render the same simulation at any size, clamping strokes to ≥ 0.75 device px, reducing samples (LOD) and hiding labels/badges under 200 px. With reduced motion (OS setting or app toggle) breathing, wobble, drift, spring overshoot and rotating dashes SHALL be removed while states stay distinguishable.

#### Scenario: mini window
- GIVEN a 118 px canvas
- THEN no labels are drawn and strokes remain visible

#### Scenario: reduced motion
- THEN membrane radius is constant in Idle and springs are monotone
