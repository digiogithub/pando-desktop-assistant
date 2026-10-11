# Pando Assistant — "Living Organism" design system

Design system for the native Pando desktop assistant (Rust + GPUI). It extends the
**Pando brand v1** (`../../pando/assets/pando-brand-v1/README.md`) with one central idea:

> The assistant is not a dashboard. It is **one living organism** — the Pando grove: a single
> root system that looks like a forest. The main agent (mesnada) is the mother cell; every
> agent it coordinates is a daughter cell joined to the body by a pseudopod. Nothing on
> screen is a static drawing: every frame is simulated.

`tokens.json` is the machine-readable source of truth. `reference/organism.html` is a
dependency-free reference implementation of the simulation (open it in a browser). The
prototype canvas this was distilled from lives in the Claude Design artifact "Pando Orbe"
(artboard *Pando Desktop · organismo vivo*).

---

## 1. Brand foundations (inherited, do not change)

| Token | Hex | Use |
|---|---|---|
| Bosque | `#0F2A20` | Window background. Every mark sits on it. |
| Marfil | `#F4F1E8` | Text, membrane strokes. **Never pure white.** |
| Álamo | `#E9B949` | Nucleus, Pando-engine cells, accents on dark. |
| Álamo oscuro | `#C68A17` | Accent on light surfaces only. Never for text. |
| `#8F6310` | — | AA accent for text/controls if a light theme is ever built. |

- Secondary text and hairlines are **Marfil at alpha** (62 / 45 / 22 / 12 / 7 %). No greys.
- Typography: **Space Grotesk** (display, 400–600), **JetBrains Mono** (labels, streams),
  **Zen Kaku Gothic New** 300 for UI copy (fallback `system-ui`). Light weights by default.
- Use the provided SVGs in `brand/` (logo, marks, mesnada mark). Do not redraw them.

### Palette extensions (not in brand v1 — adopted as default, pending product-owner sign-off)

| Token | Hex | Use |
|---|---|---|
| Claude blue | `#8FB8DE` | Any Claude Code engine (ACP-spawned or external session). |
| Bermellón | `#E5533D` | Error nucleus / error badge. |

Both were chosen to differ from Álamo in **hue and lightness** so engine and error read
without colour vision, and to sit at similar chroma to the brand accent.

---

## 2. Anatomy of the organism

Everything is drawn on a square canvas; sizes are fractions of its width `W`
(see `tokens.organism`).

```
            habitat ring (0.47 W, Marfil 7%)
     ┌───────────────────────────────────────┐
     │        ● daughter cell  (0.03 W)      │
     │         ╲ pseudopod                   │
     │   ●──────╲        ╭────╮              │
     │           ╲       │ body (0.13 W)     │
     │            ╲─────▶│  · nucleus (gold) │
     │                   │  · organelles     │
     │   ●───────────────╰────╯──────────●   │
     │                 ring radius 0.36 W    │
     └───────────────────────────────────────┘
```

| Part | What it represents | How it is drawn |
|---|---|---|
| **Body / membrane** | mesnada, the orchestrator | Closed polyline of 160 samples. Radius per angle = base × (1 + noise) + voice ripple + bulge toward each cell. Fill Marfil 6 %, halo stroke 7 px @ 16 %, crisp stroke 1.5 px @ 90 %. |
| **Nucleus** | attention / the voice | Gold disc 0.028 W with a 1.35× translucent corona and a 1.9× dashed orbit. **Follows the pointer** (max 0.04 W, lerp 0.08). Scales with the voice envelope. |
| **Organelles** | background activity | 70 particles drifting inside the membrane (clipped). Speed ×3 while thinking. |
| **Pseudopod** | the link to an agent | From the membrane edge to the cell edge. 18-sample curve with a perpendicular sine wobble enveloped by `sin(πs)`. Two layers: halo 9 px @ 14 % + engine-coloured core 1.6 px @ 75 %. |
| **Vesicles** | messages in flight | 7 dots per pseudopod moving along it: **outward** while the agent works, **inward** (Marfil) while it waits for the user. |
| **Daughter cell** | one agent | Noisy circle 0.03 W, Marfil stroke, nucleus = engine colour at 0.34×. Label below in JetBrains Mono 300 11 px. |
| **Waves** | speech / birth | Expanding Marfil circles from the body edge, decaying over ~1.8 s. |

### Engine encoding (the cell's nucleus)

| Engine | Nucleus | Membrane | Can you talk to it? |
|---|---|---|---|
| Pando instance | solid Álamo | solid | yes |
| Claude Code via ACP (spawned by mesnada) | solid Claude blue | solid | yes |
| Claude Code external session (user-opened) | **hollow** Claude blue | solid | yes (channel) |
| CLI command | hollow Marfil | **dashed** ("spore") | no — output only |

Rule: **fill = you can address it; colour = who it is.**

### Status encoding (how the cell moves)

| Status | Nucleus | Pseudopod | Badge |
|---|---|---|---|
| pending (queued) | still, cell at 60 % alpha | vesicles flow outward | — |
| working | beats (3.2 rad/s ≈ 0.5 Hz, +18 %) | vesicles flow outward | — |
| waiting for user | blinks (square on `sin 5t`, ≈ 0.8 Hz) | vesicles flow inward, Marfil | `?` |
| paused | cell at 35 % alpha | still, 30 % | `II` |
| done | Marfil 45 % | still | `✓` |
| error | Bermellón, tremor 18 rad/s (≈ 2.9 Hz, ±8 %) | still | `!` (Bermellón) |

Paused, done and error cells are "dead" for motion: their pseudopod is still and their
membrane bulge uses `deadFactor` (0.35).

Status is never colour alone: every status also changes motion or opacity.

---

## 3. States of the body (what the voice pipeline drives)

| State | Trigger | Body | Nucleus | Extra |
|---|---|---|---|---|
| `idle` | nothing queued | breathes (≈ 4.8 s period, `sin 1.3t`) | follows pointer, slow pulse ±4 % | — |
| `listening` | push-to-talk held / VAD open | +6 %, membrane trembles with **mic RMS** | grows with the envelope (`+0.9·amp`) | — |
| `thinking` | request sent, no first token | −3 % | `+0.9·amp` (amp 0.18) with a ±12 % pulse at 7 rad/s, dashed orbit spins, 3 satellites orbit | organelles ×3, vesicles ×2.2 |
| `speaking` | TTS playing | +8 %, membrane ripples with **TTS RMS** | grows with the envelope (`+0.9·amp`) | waves at 2.2/s (Poisson) |
| `delegating` | a new task appears | bulge toward birth angle | — | **mitosis**: new cell buds from the body |

`delegating` is a 1.5 s overlay on top of the current state: it adds the birth bulge and the
wave but does not change `bodyRadiusByState`, then the previous state resumes.

The voice envelope in the prototype is synthetic (`tokens.organism.voiceEnvelope`). In the
app it must be the real RMS of the capture buffer (listening) or of the TTS output buffer
(speaking), normalised 0..1 and smoothed with `amplitudeLerpPerSecond`.

### Mitosis (cell birth)

1. New cell spawns at the body centre with `born = now`.
2. For 2.2 s the membrane bulge toward its angle gets `+1.6` extra, decaying linearly.
3. The cell's radius grows 0→1 over 1.6 s; its position is a **damped spring** toward its ring slot
   with a softer `k` (2.2 vs 3.5) so it drifts out instead of snapping.
4. A wave is emitted once (by the birth itself, not by the state change).

### Death

Cells are removed when their task disappears; fade alpha over 0.6 s and retract the pseudopod
(scale its length to 0) before deleting. Cancelled tasks show as done for 10 s and then die.
A dying cell that reappears is revived from its current state.

---

## 4. Motion rules

- **Nothing is ever perfectly still.** Every element has at least breathing or noise.
- **Springs, not tweens.** Position changes are damped springs
  (`k`, `damping`, `gain` in tokens). Scale/size morphs use the spring ease
  `cubic-bezier(.34,1.9,.5,1)` with slight overshoot.
- **Periodic noise, not random.** Membrane/cell noise is a sum of sines in `(angle, time)`
  so closed shapes stay closed and motion is smooth across frames.
- **Voice is the master clock.** Listening/speaking amplitude modulates membrane, nucleus,
  wobble and vesicle speed together.
- Target 60 fps; the whole scene is ~2 k path points per frame. If the window is hidden,
  pause the clock (do not accumulate).
- Respect reduced-motion: keep state changes, drop breathing/wobble and spring overshoot.

---

## 5. Chrome around the organism

- Header: wordmark (Space Grotesk 600 21 px) · project pill (`proyecto · <name> · why`,
  with the decision confidence in Álamo mono) · right-aligned mono status
  (`stt · whistle`, `tts · piper es_ES`, `decisión · <model>`, `● local` in Álamo).
- Below the canvas: state label (Space Grotesk 500 19 px) + hint (13 px, Marfil 62 %),
  push-to-talk mic button (60 px circle, hairline border; Álamo when active) and a legend.
- Side panel (max 500 px): three cards — *try an example* / *voice → decision → action* /
  *cell* — plus the manual state switcher. Cards: Marfil 2.5 % fill, Marfil 9 % hairline,
  radius 22, padding 20×22.
- Buttons: pill, transparent, hairline Marfil 18 %, min-height 44; active = Álamo
  border + text. Hover lifts 1 px with the spring ease.
- Decision rows: question (Marfil 62 %) · answer (Marfil) · probability (Álamo mono) over a
  1 px bar filled to `p`.
- Result line: mono, left border 1 px Álamo.
- Floating mini window (380×440, radius 32): same organism at 118 px, state + transcript,
  `⌥ space` hint. Always-on-top, click to expand.

---

## 6. Implementing in GPUI

- Render the organism with `canvas()` (GPUI's immediate-mode element), painting the
  backend-neutral `DrawList` from `crates/organism` with `PathBuilder` + `window.paint_path`.
  `tiny-skia` rasterisation is only for golden tests and as a fallback
  ([ADR 0006](../docs/adr/0006-tiny-skia-tests-and-fallback-only.md)). Hold the simulation
  in an `Entity<Organism>` (GPUI through `gpui-kit =0.7.1`; `Model<T>` and `ViewContext` are
  stale APIs) and request the next frame with **`window.request_animation_frame()`** while
  the window is visible (`window.is_visible()`); stop requesting frames when hidden and
  reset the clock on show.
- Keep **simulation** (pure Rust, no GPUI types: `Organism::step(dt, inputs)`) separate from
  **drawing** (`Organism::shape(&Scene) -> DrawList`), so it can be unit-tested and reused in
  the mini window at another scale.
- Inputs per frame: `state`, `mic_rms`, `tts_rms`, `pointer`, `cells: Vec<CellInput>`.
- Noise helper: `fn noise(a: f32, t: f32, k: f32) -> f32` = Σ wᵢ·sin(fᵢ·a + gᵢ·t + k·cᵢ)
  with the harmonics in tokens; no Perlin needed.
- GPUI has no path clipping (rect content masks only) and no dash offset: organelle
  containment is analytic and dashing is done in the organism crate.
- `cx.reduce_motion()` is not wired to the OS in gpui-pre: the app reads the OS setting and
  calls `set_reduce_motion`.
- Hit-testing: nearest cell within 40 px of the pointer. Hover scales the cell 1.25×.
- Fonts: bundle Space Grotesk, JetBrains Mono and Zen Kaku Gothic New (OFL) in the binary so
  the look is identical on Linux/macOS/Windows.

---

## 7. Design decisions (README ↔ reference, PDA-T-0017, 2026-10-12)

`reference/organism.html` and this README disagreed in places
(`docs/research/organism-port.md` §1.19 and §5). Rule applied: **the reference wins**
unless a decision below says otherwise; `tokens.json` and the reference were updated to
match. Items marked *(PO)* are recommended defaults awaiting product-owner sign-off.

| # | Question | Decision |
|---|---|---|
| 1 | Breathing period 4.8 s (code) vs 7 s (README) | **4.8 s** (`sin 1.3t`), token `membraneNoise.breathRadPerSecond: 1.3` |
| 2 | Hz vs rad/s | Every angular speed is **rad/s**. `cellBeat.hz` renamed to `cellBeat.radPerSecond` (3.2 ≈ 0.51 Hz); new `cellBlink` (5 rad/s square ≈ 0.8 Hz) and `errorTremor` (18 rad/s ≈ 2.9 Hz) tokens. Visual speed is unchanged |
| 3 | Nucleus scale per state (+50 % / −20 % / +80 %) | No fixed per-state scale: `nr = nucleusRadius·W·(1 + 0.9·amp + pulse)`, pulse ±12 % at 7 rad/s while thinking, ±4 % at 2 rad/s otherwise (token `nucleusScale`). Real mic/TTS RMS drives `amp` |
| 4 | Idle ripple every 3.5 s, listening ripples 1.8 s | **Dropped.** Waves are emitted only on entering speaking or delegating, on cell birth, and at 2.2/s while speaking (`waves.emitOn`). Breathing already keeps idle alive; fewer idle wakeups |
| 5 | "Elastic pulse 0.48 s" while speaking | **Dropped**; speaking motion comes from the TTS RMS ripple |
| 6 | Cell growth 1.5 s (tokens) vs 1.6 s (code) | **1.6 s**; `motion.durations.cellBirth` = 1.6 |
| 7 | Error cells not dimmed in the membrane bulge | **Error is dead for the bulge too** (paused, done, error use `deadFactor`). Reference fixed to use `dead()` |
| 8 | Nucleus follow lerp per frame | Keep 0.08 per 60 Hz frame, applied frame-rate independently: `1-(1-0.08)^(60·dt)` |
| 9 | Stable slots vs reflow | **Stable slots.** A cell keeps its slot index for life; slot angle `−π/2 + 2π·i/slots + 0.15` with `slots = max(ring.minSlots 6, live cells)`. Gaps left by dead cells are compacted (in `created_at` order) after `ring.compactAfterSeconds` (10 s) without births or deaths; springs make every move smooth |
| 10 | Maximum cells / overflow *(PO)* | **12 cells on the ring** (`ring.maxCells`). Beyond that, the oldest done/paused cells collapse into one hollow Marfil **`+N` cell** whose card lists them; waiting and error cells are never collapsed |
| 11 | Constant scaling in the mini window | Pixel constants scale **linearly** with `W / 640`, with floors: strokes ≥ 0.75 device px, halos ≥ 2 px, hit radius ≥ 12 px; labels and badges hidden below 200 px (`organism.scaling`) |
| 12 | `pending` tasks (docs/06 "dim 60 %") | New `pending` status: cell at 60 % alpha, nucleus still, vesicles flow outward (`status.pending`, `organism.pendingAlpha`) |
| 13 | Delegating overlay | 1.5 s overlay; adds bulge + wave, does not change `bodyRadiusByState` |
| 14 | Claude blue `#8FB8DE` and Bermellón `#E5533D` *(PO)* | **Adopted as defaults** (hue and lightness differ from Álamo, so engine and error read without colour vision). Still not part of brand v1: the product owner must sign them off before release; until then they stay flagged in `tokens.json` |

