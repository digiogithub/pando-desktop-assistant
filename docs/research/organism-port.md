# Organism port plan — `organism.html` → `crates/organism` + GPUI adapter

Sources read: `design-system/README.md`, `design-system/tokens.json`,
`design-system/reference/organism.html` (154 lines, the source of truth for behaviour),
`docs/06-ui-mapping.md`, `docs/01-strategy.md`, `design-system/brand/*.svg`, and the GPUI
version that Bitácora uses (`gpui-kit =0.7.1` → **`gpui-pre 0.3.8`** + `gpui-pre-wgpu 0.3.8`,
`gpui-pre-linux/macos/windows 0.3.8`, in `~/.cargo/registry/src/index.crates.io-*/`).
Bitácora already uses `PathBuilder::stroke` + `window.paint_path` + `request_animation_frame`
(`crates/bitacora-app/src/views/graph_view.rs:796,911`) and `tiny-skia 0.11.4` for PNG export
(`graph_view/export.rs`). `lyon_tessellation 1.0.22` and `tiny-skia` are already in that lockfile.

All line refs `L<n>` point to `reference/organism.html`.

---

## 1. Inventory of simulation elements

### 1.0 Frame loop, clock, coordinates
| Item | Detail | Ref |
|---|---|---|
| Loop | `dt = min(0.05, Δms/1000)`; `S.t += dt`; `step(dt)` then `draw()` per rAF | L142 |
| Canvas | square, logical width `W` (640 default), `C = W/2`, DPR ≤ 2 | L53 |
| Tokens | `T` = inline `DEFAULTS` (L22-34), replaced by `../tokens.json` if fetch succeeds (L36). Key names are identical to `tokens.json` | L22-36 |
| RNG | `Math.random` used in: organelle init (L52), cell `phase` (L71), vesicle flow init (L71), speaking wave emission (L66) | — |

### 1.1 Noise function
`noise(a, t, k) = Σᵢ wᵢ·sin(fᵢ·a + gᵢ·t + k·cᵢ)` with harmonics
`[[3,0.7,0.5],[5,-1.1,0.3],[8,0.45,0.2]]` (`organism.membraneNoise.harmonics`) and **hidden**
phase multipliers `cᵢ = [1, 2.1, 0.7]` (not in tokens). L55. Integer angular frequencies ⇒
periodic in `a` ⇒ closed shapes stay closed. Used by body (k=0, t) and cells (k=phase, t·1.4).

### 1.2 Voice amplitude (synthetic envelope)
L60-64. `target`: idle/delegating 0.10, thinking 0.18, listening
`0.35+0.45|sin(6.3τ)·sin(1.7τ)|`, speaking `0.30+0.50|sin(9.1τ)·cos(2.3τ)|`, where `τ = S.ampT`
accumulates only while listening/speaking. `amp += (target−amp)·min(1, dt·amplitudeLerpPerSecond=12)`.
Tokens: `organism.voiceEnvelope` (strings, informative), `motion.physics.amplitudeLerpPerSecond`.
In the app `target = mic_rms` (listening) / `tts_rms` (speaking); keep the constants 0.10/0.18 for
the other states.

### 1.3 Body / membrane — `bodyR(a)` L85-94, draw L114-115
- `base = W·bodyRadius(0.13)·bodyRadiusByState[state]` (idle 1, listening 1.06, thinking 0.97,
  speaking 1.08, delegating 1).
- `r = base·(1 + membraneNoise.amplitude(0.05)·noise(a,t,0) + membraneNoise.breath(0.012)·sin(1.3t))`
  (breath period = 4.83 s; README says "7 s" — mismatch).
- Voice ripple: `+ base·amp·voiceRipple.amplitude(0.16)·sin(11a+14t)·sin(7a−9t)` (`angular [11,7]`,
  `temporal [14,−9]`).
- Bulge toward each cell (L89-92): `d = wrap(atan2(cell−C) − a)`;
  `birth = age<birthSeconds(2.2) ? (1−age/2.2)·birthExtra(1.6) : 0`;
  `alive = status∈{paused,done} ? deadFactor(0.35) : 1` (**error is not dimmed here** although
  `dead()` includes error — inconsistency);
  `r += base·(amount(0.16)·alive + birth)·exp(−d²/(2·sigma(0.16)²))`.
- Draw: closed polyline of 160 segments (161 pts, L114); fill Marfil α .06; halo stroke 7 px Marfil
  α .16; crisp stroke 1.5 px Marfil × globalAlpha .9 (L115). Tokens `stroke.membrane/membraneHalo`.
- Cost: `bodyR` is O(cells) and is evaluated 160 + 70 (organelles) + n (pseudopods) times/frame.

### 1.4 Organelles (particles) — init L52, step L82-83, draw L116
- 70 particles `{a∈[0,2π), r∈[0,1), s=.2+.6·rand, sz=.8+1.6·rand}` (`organelles.count/sizeMin/sizeMax`;
  speed range .2-.8 hidden).
- Step: `a += s·dt·0.7·speedByState[state]` (idle 1, thinking 3, speaking 1.8, others → 1);
  `r += sin(0.9t + 3a)·dt·0.08`; clamp r to [0.05, 0.95].
- Draw: position radius `bodyR(a)·r·0.92`, disc radius `sz`, fill Marfil α `.18+.25·r`,
  **clipped to the body path** (`ctx.clip()` L116).

### 1.5 Nucleus — L118-124
- Follow: target = C + dir(pointer−C)·min(W·nucleusFollowMax(0.04), d·0.12); **lerp 0.08 per frame**
  (`nucleusFollowLerp`, frame-rate dependent, computed in `draw()`, not `step()`).
- `nr = W·nucleusRadius(0.028)·(1 + 0.9·amp + (thinking ? 0.12·sin 7t : 0.04·sin 2t))`.
  (README says listening "+50 %", thinking "−20 %", speaking "+80 %": not what the code does.)
- Orbit ring r = 1.9·nr, 1 px Álamo α .35; thinking only: dash [3,4], `lineDashOffset = −20t`.
- Corona r = 1.35·nr fill Álamo α .18; core r = nr fill Álamo.
- Satellites (thinking): 3 dots r 2 px Marfil at angle `2.4t + k·2.094`, radius 1.9·nr (L124).

### 1.6 Cells (daughter cells) — step L69-80, `cellR` L95, draw L127-140
- Ring slot: `ang = −π/2 + (2π/n)·i + 0.15`, `RC = W·ringRadius(0.36)`; drift `6·sin(0.6t+phase)` **px**
  (radial; y drift ×0.6) — slots reflow whenever n changes (docs/06 wants stable slots).
- Creation: if `a.isNew` → spawn at centre, `born = t`; else at slot, `born = −10`; `phase = rand·6.28`;
  `flow = 7 × rand` (L71).
- Spring (semi-implicit Euler, L74-76): `k = age<2.2 ? cellSpringKNewborn(2.2) : cellSpringK(3.5)`;
  `v += (target−x)·k·dt; v *= cellDampingPerSecond(0.08)^dt; x += v·dt·cellPositionGain(4)`.
- Hover: target 1 if pointer within **34 px** of centre; `hover += (t−h)·min(1, dt·hoverLerpPerSecond(10))` (L79).
- Removal: instant `delete` when the agent disappears (L81). **No death animation in the reference.**
- `cellR = W·cellRadius(0.03)·g·(1+(cellHoverScale(1.25)−1)·hover)`, `g = age<2.2 ? min(1, age/1.6) : 1`
  (README/tokens say `cellBirth` 1.5 s vs code 1.6 s).
- Membrane: 48-segment closed polyline, `r = cr·(1 + wig·noise(aa, 1.4t, phase))`, wig working .08
  else .04 (`cellNoise`). globalAlpha .35 if paused.
  - cli: fill Marfil .04, dashed [3,4] stroke Marfil .75 1.2 px.
  - other: fill Marfil .07, stroke Marfil 1.3 px (`stroke.cell`).
- Cell nucleus (L132-135): `nrr = cr·cellNucleusRatio(0.34)·beat`,
  `beat = working ? 1 + 0.18·max(0, sin(3.2t+phase))³ : 1` — **3.2 is rad/s (0.51 Hz), not Hz** as
  `cellBeat.hz` and README claim. waiting: alpha `.3 + .7·[sin(5t)>0]` (0.8 Hz, README says 5 Hz).
  done: fill Marfil .45. error: Bermellón, `nrr = cr·0.38·(1+0.08·sin 18t)` (2.9 Hz). Engine
  external/cli → hollow stroke 1.5 px, else filled. Colour = engine (pando Álamo, acp/external
  Claude blue, cli Marfil).
- Selection ring: r `cr + 7 + 1.5·sin 2t`, Álamo α .75, 1 px (L136).
- Badge (L137-138): `{done ✓, waiting ?, error !, paused II}` at `(x+.75cr, y−.75cr)`, disc r 7 fill
  Bosque, 1 px stroke (Bermellón if error else Marfil), glyph mono 400 9 px, baseline +3.
  Not affected by the paused 35 % alpha.
- Label: mono 300 11 px, Marfil α `.55+.45·hover`, centred, baseline `y + cr + 16` (L139).

### 1.7 Pseudopods — L104-110
- `ang = atan2(cell − C)`; start `r0 = bodyR(ang) − 2`; end at `cell − dir·(cr − 1)`; normal `n̂`
  (divides by `len` — NaN if 0, guard needed).
- `wob = (dead ? wobbleDead 2 : wobble 6)·(1 + amp·wobbleVoiceGain 2.5)` (px, unscaled).
- `samples 18` ⇒ 19 points: `off = sin(πs)·wob·(sin(9s − t·(dead?.6:2.4) + phase) + .5·sin(17s + 3.1t + phase))`.
- Path: moveTo p0, quadratic curves through midpoints (`quadraticCurveTo(pᵢ, mid(pᵢ,pᵢ₊₁))`), lineTo pN.
  `lineCap = 'round'` — **never reset**, so every later stroke (cell dashes, nucleus orbit dashes)
  also gets round caps in the reference.
- Layer 0 halo: Marfil α .14 (.07 dead) 9 px. Layer 1 core 1.6 px: engine colour × globalAlpha
  .75 (.3 dead); cli = Marfil α .5 × .75 with dash [3,5].

### 1.8 Vesicles — flow L77-78, draw L111
- 7 per pseudopod (`pseudopod.vesicles`), `f ∈ [0,1)`, `f = ((f + spd·dt) mod 1 + 1) mod 1`,
  `spd = working .28 | waiting −.22 | else 0`, ×2.2 while thinking (`flowSpeed`).
- Drawn only when not dead; position = linear interpolation along the 19 sample points (not the
  curve), radius `2 + 1.2·sin(πf)` px, alpha `.5+.5·sin(πf)`, colour Marfil if waiting else engine.

### 1.9 Waves — L65-67, draw L102
- Emitted (`{r:0, life:1}`) on entering speaking or delegating (L65) and as a Poisson process at
  `speakingRatePerSecond 2.2` while speaking (L66, needs RNG).
- `r += dt·W·speed(0.28)`; `life −= dt·decayPerSecond(0.55)` (life 1.82 s, cf. `durations.waveLife 1.8`).
- Draw: circle radius `W·bodyRadius + r` (static base radius, not `bodyR`), stroke Marfil α `.45·life`,
  width `1 + life`.
- README promises "ripple every 3.5 s" in idle and "ripples 1.8 s" in listening: **not in the reference**.

### 1.10 Habitat ring — L101
Circle `W·habitatRadius(0.47)`, 1 px Marfil α .07.

### 1.11 Satellites — see 1.5 (thinking only).

### 1.12 Mitosis — spawn handler L150 + birth terms
New cell `isNew` → spawn at C with `born=t` (L71), membrane bulge `+1.6·(1−age/2.2)` (L91), radius
growth `min(1, age/1.6)` (L95), soft spring k 2.2 for 2.2 s (L74). The **wave is emitted only because
the button also sets `state='delegating'`** (L65/L150) — in the port, birth should emit the wave itself.

### 1.13 Death — not implemented (L81 deletes instantly). Spec (README §3): fade alpha over 0.6 s and
retract the pseudopod (length → 0) before deleting; docs/06: cancelled → Done then fade after 10 s.

### 1.14 Hover / hit-test / pointer
- `pt()` maps client → canvas logical coords (L145); `mouseleave` → pointer None (L146).
- Hover radius 34 px (L79); click selects the nearest cell within 40 px (L147). Selection is the id `sel`.

### 1.15 Body state machine
Reference: buttons set `state` directly (L149); spawn forces `delegating` permanently (L150).
Effects per state: bodyRadiusByState, amplitude target, organelle speed, vesicle ×2.2 (thinking),
nucleus dash/satellites/pulse (thinking), waves (speaking/delegating entry + speaking Poisson).
docs/06 defines transitions (Idle→Listening→Thinking→Speaking…, barge-in) and **Delegating as a 1.5 s
transient overlay** that returns to the previous state — not in the reference.

### 1.16 Reduced motion — not implemented in the reference. README §4: keep state changes, drop
breathing/wobble/spring overshoot.

### 1.17 Draw order (must be preserved)
habitat → waves → per cell {pseudopod halo, core, vesicles} → body {fill, halo, crisp} →
organelles (clipped) → nucleus {orbit, corona, core, satellites} → per cell {membrane fill+stroke,
nucleus, selection ring, badge disc+glyph, label}.

### 1.18 Unscaled pixel constants (matter for the mini window)
drift 6, wobble 6/2, halo widths 9/7, stroke widths 1/1.2/1.3/1.5/1.6, `bodyR−2`, `cr−1`,
vesicle 2+1.2, satellites 2, selection +7/±1.5, badge r 7 and offsets, label +16, fonts 11/9,
dash [3,4]/[3,5], dash speed 20 px/s, hover 34, click 40. Wave growth and all radii scale with W.

### 1.19 Doc ↔ code discrepancies to resolve (pick the reference unless design says otherwise)
breath 4.8 s vs "7 s"; beat/blink/tremor rad/s vs "Hz"; cell growth 1.6 vs 1.5 s; nucleus scale per
state; idle/listening ripples; error not in bulge `deadFactor`; nucleus lerp per frame; ring reflow;
`pending → dim 60 %` (docs/06) has no reference; "elastic pulse 0.48 s" (speaking) absent.

---

## 2. `crates/organism` design (pure Rust, no GPUI)

Dependencies: `serde`/`serde_json` (tokens), nothing else at runtime. Dev: `proptest`, `insta`
(or hand-rolled JSON goldens), `approx`.

```
crates/organism/src/
  lib.rs        // pub use; Organism facade
  params.rs     // Params: serde from tokens.json (include_str! default) + Extras (hidden constants)
  types.rs      // BodyState, Engine, Status, CellId, CellInput, Inputs, Pointer
  rng.rs        // Pcg32 / mulberry32 (bit-identical to the JS harness), hash(id) → phase seed
  noise.rs      // fn noise(h:&Harmonics, a, t, k) -> f64
  spring.rs     // CellSpring {x,y,vx,vy}::step(target, k, damping, gain, dt) (reference Euler)
  envelope.rs   // Amplitude smoother (+ synthetic envelope for demo/tests)
  state.rs      // BodyMachine: base state + Delegating overlay (1.5 s), transition events
  sim.rs        // Organism::step(dt, &Inputs) — cells, flow, waves, organelles, nucleus, hover
  body.rs       // body_radius(angle) with cached per-cell bulge terms
  shape.rs      // Organism::shape(&Scene) -> DrawList
  draw.rs       // DrawList / Prim / Paint / Stroke / Dash / Color(rgba f32)
  layout.rs     // Scene::layout(size_px, scale_factor) → Scene {w, px (=W/640 scale), lod}
  hit.rs        // hit_test(point) -> Option<CellId>, hover targets
```

### Key types
```rust
pub enum BodyState { Idle, Listening, Thinking, Speaking, Delegating }
pub enum Engine { Pando, ClaudeAcp, ClaudeExternal, Cli }
pub enum Status { Working, Waiting, Paused, Done, Error }
pub struct CellInput { pub id: CellId, pub engine: Engine, pub status: Status,
                       pub is_new: bool, pub dim: f32 /* pending 0.6 */, pub label: SharedLabel }
pub struct Inputs<'a> { pub state: BodyState, pub mic_rms: f32, pub tts_rms: f32,
                        pub pointer: Option<[f32;2]> /* normalised 0..1 canvas space */,
                        pub selected: Option<CellId>, pub cells: &'a [CellInput],
                        pub reduced_motion: bool }
pub struct Organism { t: f64, amp: f64, amp_tau: f64, nucleus: [f64;2],
                      cells: IndexMap-like Vec<CellSim>, organelles: [Organelle; N], waves: Vec<Wave>,
                      machine: BodyMachine, rng: Pcg32, params: Arc<Params> }
struct CellSim { id, slot: u32, pos, vel, born: f64, phase: f64, hover: f64,
                 flow: [f64; 7], life: Life /* Alive | Dying{since} */, input: CellInput }
```
- **Units**: simulate in a *normalised* canvas of width `W_REF = 640` (the reference size) so all
  pixel constants are literal; `Scene` applies `scale = W_px/640` (with per-primitive min widths).
  This keeps golden parity trivial and makes the mini window the same code.
- **Precision**: f64 inside (`t` grows unbounded; `sin(18t)` in f32 degrades after hours), f32 in the
  draw list.
- **Phase/slot stability**: `phase = hash(id)` (not RNG) so main + mini window agree and restarts
  don't reshuffle; slots assigned on first sight, freed on death; ring angle from slot index over
  `max(n, slots_in_use)` (fix for reflow) behind a `parity` flag.
- **Frame-rate independence**: nucleus lerp → `1 − (1−0.08)^(dt·60)` (identical at 60 fps).
  Waves Poisson uses `rng`. `dt` clamped to 0.05 like the reference; optional substeps if dt > 1/30.
- **Death**: `Life::Dying{since}` for 0.6 s: alpha 1→0, pseudopod length factor 1→0, bulge fades,
  then removed. Cells reappearing while dying are revived.
- **Birth**: emits a wave itself (not only via Delegating); Delegating overlay is pushed by the app
  on task creation (1.5 s), state machine returns to the base state.
- **Reduced motion** (`inputs.reduced_motion`): breath/noise/ripple/wobble amplitudes → 0 (or ×0.15),
  drift 0, springs critically damped (no overshoot; snap-ish), organelles static, satellites/dash
  rotation static, waves replaced by a single non-expanding fade; status still encoded via
  opacity/badges (blink/beat → steady alpha differences).

### `step(dt, inputs)` order (mirrors L58-84 + L118-119)
amplitude → state machine/waves emit → wave advance/cull → cell reconcile (create/dying) → per-cell
spring, flow, hover → organelles → nucleus follow. Everything that `draw()` mutates in JS (nucleus)
moves into `step`.

### `shape(&Scene) -> DrawList`
```rust
pub enum Prim {
  Poly { pts: Range<u32>, closed: bool, fill: Option<Rgba>, strokes: SmallVec<[Stroke;2]> },
  Curve { pts: Range<u32> },            // pseudopod: quadratic-midpoint spline, like L108
  Circle { c: [f32;2], r: f32, fill: Option<Rgba>, stroke: Option<Stroke> },
  Discs { centers/radii range, color: Rgba } // batched organelles/vesicles by alpha bucket
  Text { text: LabelRef, pos, size: f32, weight: u16, color: Rgba, align: Center },
  ClipBegin(PolyRef) / ClipEnd,         // optional; default shape() pre-clips analytically
}
pub struct Stroke { width: f32, color: Rgba, cap: Cap, dash: Option<Dash { on: f32, off: f32, phase: f32 }> }
pub struct DrawList { points: Vec<[f32;2]>, prims: Vec<Prim> }   // reused buffers, no per-frame alloc
```
- Colours resolved in the crate (tokens → `Rgba`), globalAlpha folded into each colour's alpha.
- `Curve` stays a curve primitive (renderers flatten); a helper `flatten_curve(tol)` exists for
  tiny-skia/tests.
- `Dash` is resolved by a crate helper `dash_polyline(&[pt], dash) -> Vec<Range>` so both renderers
  get the same dashes incl. **phase** (GPUI's `dash_array` has no offset).
- Organelle clip: analytic — clamp `rr ≤ bodyR(a)·0.92 − sz` (centre radius always ≤ 0.92·bodyR at
  the same angle, so only the disc rim can poke out). Emits `ClipBegin` only for the raster backend
  if exact parity is wanted.
- LOD from `Scene`: body samples 160/96, cell samples 48/32, organelles 70/30, labels+badges off
  under 200 px (docs/06), min stroke 0.75 px device.
- Accessibility side output: `Organism::semantics()` → list of cells with bounds/label/status for
  GPUI hitboxes/tooltips.

### Test strategy
1. **JS golden harness** (`design-system/reference/harness.mjs`, Node, no deps): loads the
   `<script>` from `organism.html`, stubs `document/canvas/fetch/requestAnimationFrame`, replaces
   `Math.random` with the same `mulberry32(seed)` as `rng.rs`, injects a fixed agent list and a
   scripted timeline (state changes, spawn, kill, pointer path), drives `step/draw` at fixed dt
   (1/60), and records ctx calls (path points, arc, stroke/fill style, lineWidth, dash, alpha, text)
   as JSON per frame for frames {0, 1, 30, 120, 600}. Rust test replays the same timeline and
   compares the `DrawList` (points ±1e-3 px, alpha ±1e-4) in `parity` mode. Golden files committed
   under `crates/organism/tests/golden/`.
2. **Property tests** (proptest): for random inputs/dt ∈ (0, 0.05], cell lists 0..32, all states:
   no NaN/inf in draw list; body polyline closed (first==last within ε) and radius > 0; cells within
   habitat after settling; vesicle `f ∈ [0,1)`; waves pruned; `step(dt)` twice ≈ `step(2dt)` within
   tolerance for springs (dt-stability), and no energy blow-up at dt = 0.05 for 10⁴ steps; zero-length
   pseudopod does not NaN; removing all cells while dying is safe.
3. **Unit tests**: noise periodicity `noise(a+2π)=noise(a)`; bulge peak at the cell angle; state
   machine transitions table from docs/06; reduced-motion zeroes breathing.
4. **Raster snapshot** (feature `raster`): tiny-skia render of goldens → PNG compared with a
   Chrome screenshot of the harness at the same frame (SSIM ≥ 0.97) — the visual-parity gate.
5. **Bench** (`criterion`): `step+shape` for 6 and 32 cells, target < 0.3 ms.

---

## 3. GPUI rendering adapter (`crates/app/src/organism_view.rs`)

### Available in gpui-pre 0.3.8 (verified in source)
| Need | API | Notes |
|---|---|---|
| Custom element | `canvas(prepaint, paint)` (`elements/canvas.rs`) | Bitácora pattern: store bounds in `Rc<Cell<Bounds>>` |
| Paths | `PathBuilder::{fill(), stroke(px)}`, `.with_style(PathStyle::Stroke(StrokeOptions…))` for caps/joins, `move_to/line_to/curve_to(to, ctrl)` (quadratic), `cubic_bezier_to`, `arc_to`, `add_polygon`, `close`, `dash_array(&[Pixels])`, `build()` → lyon tessellation (`path_builder.rs`) | **No dash offset**; dashes measured per call; u16 indices per path (≤ 65k verts) |
| Paint path | `window.paint_path(path, impl Into<Background>)` — one colour/background per path, element opacity applied | Rendered with 4× MSAA into an intermediate texture (`wgpu_renderer.rs:1538, 1836`) |
| Quads | `window.paint_quad(fill(bounds, color).corner_radii(r))` — SDF-AA rounded rects, can draw discs | Different primitive kind ⇒ breaks the path batch (see perf) |
| Clipping | `window.with_content_mask(Some(ContentMask{bounds}))` | **Rect only**; no path clip, no stencil |
| Opacity | per-colour alpha; `element_opacity` is crate-private (`with_element_opacity` pub(crate)); `div().opacity()` for whole element | Fold alpha into colours |
| Blur / shadow | `paint_drop_shadows` (rounded-rect blur only); `WindowBackgroundAppearance::Blurred` (window backdrop) | No path blur; the reference doesn't need blur (halos are wide translucent strokes) |
| Text | `window.text_system().shape_line(text, size, runs, None)` → `ShapedLine::paint(origin, line_height, TextAlign::Center, None, window, cx)` | Bundle JetBrains Mono for ✓ ? ! II glyphs |
| Images | `window.paint_image(bounds, image_bounds, corners, Arc<RenderImage>, frame, grayscale)`; `RenderImage::new(frames)` is **BGRA**; `window.drop_image(arc)` to evict from atlas | New ImageId per frame ⇒ must drop the previous frame's image |
| SVG | `window.paint_svg(...)` = **monochrome alpha mask** only; `img("…svg")` renders full-colour via resvg | Brand SVGs are 2-colour (Marfil + Álamo on Bosque) ⇒ use `img()`/`svg()` per layer, not `paint_svg` |
| Frames | `window.request_animation_frame()` (notifies current view next frame), `on_next_frame(cb)` | Docs: check `cx.reduce_motion()` for decorative motion |
| Visibility | `window.is_visible()` (false when minimised/occluded/sleeping display), `observe_window_visibility(cb)`, `is_window_active()` | |
| Reduced motion | `cx.reduce_motion()` / `set_reduce_motion` | **Default false, not read from the OS** — app must read GNOME `enable-animations`, macOS `accessibilityDisplayShouldReduceMotion`, Windows `SPI_GETCLIENTAREAANIMATION` + user setting |
| Springs | `gpui::spring::SpringConfig::step` (analytic, frame-rate independent) | Useful for chrome hover lifts; organism keeps the reference Euler for parity |
| Windows | `WindowOptions { kind: WindowKind::{Normal, PopUp, Floating, LayerShell(..) (linux+wayland feature)}, window_background: Transparent/Blurred, window_decorations: Client/Server, titlebar: None, is_movable, inactive_frame_interval (default 33 ms!), focus, show }` | See mini window below |

### Mapping primitives
| Prim | GPUI | tiny-skia |
|---|---|---|
| closed Poly fill | `PathBuilder::fill()` + `add_polygon(closed)` | `fill_path` |
| Poly stroke (halo + crisp) | two `PathBuilder::stroke(w)` builds of the same points, painted in order (halo first) | `stroke_path` |
| Curve | `move_to` + `curve_to(mid, ctrl)` × N + `line_to`, `StrokeOptions::with_line_cap(Round)` | `quad_to` |
| Dashed stroke | **crate-side dashing** (`dash_polyline` with phase) → one stroke path with many sub-paths (`move_to/line_to` per dash) — avoids lyon measurement per call and supports the orbit's `−20t` offset | `StrokeDash::new(arr, phase)` |
| Circle (fill) | path: `add_polygon` of 16-48 segment circle (segments by radius) — keep in the path batch; or `paint_quad` disc when the batch boundary is acceptable | `push_circle` |
| Discs batch (organelles/vesicles) | 1 fill path with many circular sub-paths per alpha bucket (quantise alpha to 8 levels) | per disc |
| Text | `shape_line` cached per (label, size, alpha bucket) in a `HashMap`, painted after all paths | skipped (draw labels with GPUI over the image) |
| Clip | analytic in crate (default); exact path clip only possible in tiny-skia (`Mask`) | `Mask::from path` |

Stroke-with-alpha caveat: lyon stroke tessellation can produce overlapping triangles at tight
joins; with MSAA into the intermediate these double-blend only where triangles overlap. Body/cell
polylines are smooth (160/48 segments) so this is invisible; if it shows on pseudopod halos (9 px on a
wavy curve), use `LineJoin::Round` + tolerance 0.1, or render halos at full alpha into the path and
apply alpha via colour (overlap still blends twice) — acceptable; verify in the visual check.

### Batching / performance budget
- GPUI orders overlapping primitives by bounds (`scene.rs insert_primitive`); consecutive paths form
  one `PrimitiveBatch::Paths` = **one extra render pass** (rasterise to a 4× MSAA intermediate, then
  composite the union bounds). Every switch path→quad/text→path starts another pass. So emit **all
  geometry as paths in draw order, then all text** (labels/badge glyphs last; badge discs stay paths
  in order). Expected: 2 path passes + 1 text batch per frame.
- Do **not** wrap the organism in `paint_layer`: same-order paths are composited per-path bounds from
  the shared intermediate and overlapping bounds would double-composite.
- Counts for 6 cells: body 161 pts, cells 6×49, pseudopods 6×19 (flattened by lyon ~3-4×), circles
  ≈ 12 + 6×(nucleus, selection, badge) + 70 organelles + 42 vesicles. ~2-3 k points, ~100-150
  `paint_path` calls → bucketed to ~60. lyon tessellation budget ≤ 2 ms/frame; `step+shape` ≤ 0.3 ms;
  text cached. Target ≤ 4 ms CPU/frame at 640 px, 60 fps; at 32 cells ≤ 8 ms.
- Reuse buffers (DrawList, point Vecs); avoid per-frame `String` allocation for labels.
- Measure with `gpui` `profiler` feature (gpui-kit exposes `profiler`) and a frame-time overlay.

### tiny-skia fallback (feature `raster`)
Render the DrawList into a reused `Pixmap` (logical size × scale_factor), convert RGBA premultiplied
→ BGRA (straight alpha? check `RenderImage` expects premultiplied BGRA in the wgpu sprite shader —
verify with a test pattern), wrap in `Arc<RenderImage>`, `paint_image`, and `drop_image` the previous
frame's Arc. Upload cost ≈ 1280²×4 = 6.5 MB/frame at 2× — fine for the 118 px mini window, marginal
for full-screen 4K. Labels still drawn with GPUI text over the image. Also used for golden PNGs and
offline exports (app icon frames, README GIF).

### Frame driving, visibility, scaling
- `OrganismView` (Entity) owns `Organism`, `last_instant`. In `render`: `dt = now − last` clamped
  0.05; `organism.step(dt, inputs)`; `canvas(...)` paints `organism.shape(scene)`; then
  `if window.is_visible() && animating { window.request_animation_frame() }`.
- Hidden: `observe_window_visibility` → stop requesting frames; on visible reset `last_instant = now`
  (no accumulated dt). With `reduce_motion`, drop to event-driven redraw + 10 fps idle tick.
- `inactive_frame_interval`: default 33 ms throttles unfocused windows to 30 fps — set `None` for
  the main window if 60 fps unfocused is required; keep 33 ms (or 50 ms) for the mini window to save
  power.
- Pointer: `on_mouse_move` on the canvas div → convert to normalised canvas coords; `on_mouse_up` →
  `hit_test` (40 px × scale); hover radius 34 px × scale. Use GPUI hitbox (`insert_hitbox`) for the
  cursor style.
- Mini window (380×440, organism 118 px): `Scene::layout(118, sf)` ⇒ scale 0.184: strokes clamped to
  ≥ 0.75 device px, halos ≥ 2 px, labels/badges hidden, LOD on. Window: `titlebar: None`,
  `window_decorations: Some(Client)`, `window_background: Transparent`, radius 32 drawn as a Bosque
  rounded quad. Always-on-top: macOS/Windows `WindowKind::PopUp` (topmost); X11 `PopUp` =
  override-redirect + `_NET_WM_WINDOW_TYPE_NOTIFICATION` (no WM move/focus — must implement own drag
  and keyboard focus may not work); Wayland has **no** always-on-top for toplevels → `LayerShell`
  (overlay layer, needs the `wayland` feature and compositor support, not GNOME/Mutter).
- Brand marks: draw with `img()`/`svg()` elements in the chrome, never re-drawn in the organism.

---

## 4. Implementation tasks (Fibonacci points)

| # | Task | Pts | Acceptance criteria |
|---|---|---|---|
| T1 | Workspace + `crates/organism` skeleton, `Params` from `tokens.json` (`include_str!`) + `Extras` for hidden constants, colour parsing | 2 | `cargo test` loads tokens; round-trip of every token key used in §1; clippy clean |
| T2 | `rng` (mulberry32 bit-exact with JS), `noise`, `spring`, `envelope` | 2 | unit tests: noise periodic in 2π, RNG sequence equals JS for seed 1..3 |
| T3 | JS golden harness (`harness.mjs`) + scripted timeline + JSON goldens | 5 | `node harness.mjs` produces deterministic goldens (two runs identical); covers all 5 states, 4 engines, 5 statuses, spawn/kill, pointer |
| T4 | `sim.rs` step: amplitude, waves, cells (spring/flow/hover), organelles, nucleus (moved from draw) | 5 | parity mode matches goldens' state at frames 1/30/120/600 within 1e-6 |
| T5 | `body.rs` + `shape.rs` DrawList for all elements in §1.17 order, crate-side dashing with phase, curve primitive | 8 | DrawList parity vs goldens (pts ±1e-3 px, alpha ±1e-4); prim count per frame stable |
| T6 | Property tests + dt-stability + bench | 3 | proptest 10k cases no NaN; closed shapes; `step+shape` < 0.3 ms (6 cells) / < 1 ms (32) |
| T7 | Lifecycle beyond the reference: stable slots, death (0.6 s fade + retract), birth wave, Delegating overlay 1.5 s, `dim` for pending, state machine per docs/06 | 5 | unit tests per transition table; no slot jump on add/remove; dying cell revivable |
| T8 | Reduced-motion profile + LOD/`Scene::layout` scaling (min widths, hide labels < 200 px) | 3 | reduced motion: zero breathing/wobble, no overshoot (monotone spring); 118 px snapshot legible |
| T9 | GPUI adapter: `OrganismView`, canvas paint of all prims as paths, text after, label cache, pointer/hover/click, rAF + visibility pause | 8 | demo app with fake agents + keyboard state switcher; 60 fps at 640 px on Linux (X11 + Wayland) with 6 and 32 cells; frame CPU ≤ 4 ms (6 cells); hidden window → 0 frames/s |
| T10 | Visual parity check: side-by-side capture vs Chrome running `organism.html`/harness at fixed seed+frame | 3 | SSIM ≥ 0.97 on frames for each state; manual review sign-off by design |
| T11 | tiny-skia raster backend (feature `raster`) + golden PNGs + BGRA/premultiply test + `drop_image` per frame | 5 | raster PNG ≈ GPUI capture (SSIM ≥ 0.95); no atlas growth over 10 min (memory flat) |
| T12 | Mini window: transparent/borderless, always-on-top per platform, drag, 118 px organism, inactive frame interval | 5 | stays on top on macOS/Windows/X11; Wayland behaviour documented (LayerShell or normal); ≤ 2 % CPU idle |
| T13 | OS reduced-motion detection (GNOME/macOS/Windows) → `cx.set_reduce_motion` + settings toggle | 2 | toggling OS setting updates organism without restart |
| T14 | Perf hardening: alpha-bucketed disc batching, buffer reuse, profiler overlay; optional quad discs if passes are the bottleneck | 3 | 4K window 60 fps on integrated GPU; ≤ 3 path passes/frame |

Total ≈ 59 pts. Critical path for milestone 1: T1→T2→T3→T4→T5→T9→T10 (≈ 33 pts).

---

## 5. Risks and open questions

**Risks**
1. **Per-frame lyon tessellation cost** (dashes measured inside `PathBuilder::build`, curves
   flattened) — mitigated by crate-side dashing, alpha bucketing, fewer paths; fallback tiny-skia.
2. **Render-pass splitting**: interleaving quads/text with paths multiplies MSAA intermediate passes;
   design depends on "all paths, then text" ordering. Text-over-badge z-order nuance (label of cell A
   under cell B) is accepted.
3. **No path clipping** in GPUI — relying on analytic organelle containment; exact clip only in raster.
4. **Translucent stroke overlaps** (lyon join triangles) may produce slightly darker spots vs Canvas.
5. **Always-on-top on Linux**: X11 PopUp is override-redirect (focus/drag issues); Wayland needs
   LayerShell (not supported by Mutter/GNOME) — mini window may be a normal window on GNOME Wayland.
6. **`inactive_frame_interval` default 33 ms** silently caps unfocused windows at 30 fps.
7. **`cx.reduce_motion()` is not wired to the OS** in gpui-pre — we own detection.
8. **RenderImage atlas leak** if the raster path forgets `drop_image`; BGRA/premultiplied format must be
   verified.
9. Golden parity f64(JS) vs Rust: transcendental `sin/exp` may differ in the last ulp — tolerances,
   not bit equality.
10. gpui-pre is a moving pre-release (`=0.3.8` via gpui-kit 0.7.1 pin): API churn on upgrade
   (PathBuilder/canvas signatures) — keep the adapter thin.

**Open questions (need design/product sign-off)**
1. Which wins where README and reference disagree (§1.19): breath period, Hz vs rad/s, nucleus scale
   per state, idle/listening ripples, cell growth 1.5 vs 1.6 s, error in bulge dead factor?
2. Should the pixel constants scale with W (mini window) linearly or with a floor? Proposed: linear
   with min stroke widths and LOD.
3. Stable ring slots vs reference reflow (docs/06 asks stable): gaps left by dead cells — compact
   after N seconds?
4. Max cell count / overflow (> 12 cells crowd the ring): second ring, clustering, or "+N" cell?
5. Real RMS normalisation (dBFS → 0..1 curve, noise gate) belongs to voice crates — confirm the
   organism receives already-smoothed 0..1 and keeps its own `amplitudeLerpPerSecond` smoothing.
6. Delegating as overlay: does it also change `bodyRadiusByState` (1.0) while Thinking/Speaking, or
   only add bulge/wave?
7. Accessibility: expose cells as GPUI hitboxes with tooltips/keyboard focus (Tab cycling cells)?
8. Is the tiny-skia path a real runtime fallback (feature flag, auto on low-end GPUs) or test-only?
9. Palette extensions (Claude blue, Bermellón) still pending brand sign-off.
