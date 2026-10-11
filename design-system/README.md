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

### Proposed palette extensions (not in brand v1 — needs sign-off)

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
| working | beats (3.2 Hz, +18 %) | vesicles flow outward | — |
| waiting for user | blinks (5 Hz square) | vesicles flow inward, Marfil | `?` |
| paused | cell at 35 % alpha | still, 30 % | `II` |
| done | Marfil 45 % | still | `✓` |
| error | Bermellón, 18 Hz tremor | still | `!` (Bermellón) |

Status is never colour alone: every status also changes motion or opacity.

---

## 3. States of the body (what the voice pipeline drives)

| State | Trigger | Body | Nucleus | Extra |
|---|---|---|---|---|
| `idle` | nothing queued | breathes (7 s), ripple every 3.5 s | follows pointer | — |
| `listening` | push-to-talk held / VAD open | +6 %, membrane trembles with **mic RMS** | +50 % with envelope | ripples 1.8 s |
| `thinking` | request sent, no first token | −3 % | −20 %, dashed orbit spins, 3 satellites orbit | organelles ×3, vesicles ×2.2 |
| `speaking` | TTS playing | +8 %, elastic pulse 0.48 s | +80 % with **TTS RMS** | waves at 2.2/s |
| `delegating` | `orchestrator/tasks` created | bulge toward birth angle | — | **mitosis**: new cell buds from the body |

The voice envelope in the prototype is synthetic (`tokens.organism.voiceEnvelope`). In the
app it must be the real RMS of the capture buffer (listening) or of the TTS output buffer
(speaking), normalised 0..1 and smoothed with `amplitudeLerpPerSecond`.

### Mitosis (cell birth)

1. New cell spawns at the body centre with `born = now`.
2. For 2.2 s the membrane bulge toward its angle gets `+1.6` extra, decaying linearly.
3. The cell's radius grows 0→1 over 1.6 s; its position is a **damped spring** toward its ring slot
   with a softer `k` (2.2 vs 3.5) so it drifts out instead of snapping.
4. A wave is emitted once.

### Death

Cells are removed when their task disappears; fade alpha over 0.6 s and retract the pseudopod
(scale its length to 0) before deleting.

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

- Render the organism with `canvas()` (GPUI's immediate-mode element) and a
  `gpui::Path`/`PathBuilder` per stroke, or rasterise with `tiny-skia` into an `ImageSource`
  and blit — the prototype is ~5 paths per cell plus the body, so either works. Use a
  `Model<Organism>` holding the simulation state and request a redraw each frame
  (`cx.request_animation_frame()` / a 16 ms timer) while the window is visible.
- Keep **simulation** (pure Rust, no GPUI types: `Organism::step(dt, inputs)`) separate from
  **drawing** (`Organism::paint(&mut PathBuilder…)`), so it can be unit-tested and reused in
  the mini window at another scale.
- Inputs per frame: `state`, `mic_rms`, `tts_rms`, `pointer`, `cells: Vec<CellInput>`.
- Noise helper: `fn noise(a: f32, t: f32, k: f32) -> f32` = Σ wᵢ·sin(fᵢ·a + gᵢ·t + k·cᵢ)
  with the harmonics in tokens; no Perlin needed.
- Hit-testing: nearest cell within 40 px of the pointer. Hover scales the cell 1.25×.
- Fonts: bundle Space Grotesk, JetBrains Mono and Zen Kaku Gothic New (OFL) in the binary so
  the look is identical on Linux/macOS/Windows.
