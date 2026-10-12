# Pando Desktop Assistant

A fully native desktop assistant for [Pando](../pando), written in Rust with
[GPUI](https://github.com/longbridge/gpui-component) — no webview, every pixel rendered
natively.

It has two faces that share one connection to a running Pando instance:

- **The organism** — a voice-first visual assistant. Mesnada (Pando's orchestrator) is a
  living mother cell; every agent it runs is a daughter cell joined by a pseudopod. You
  hold a key (or, later, say *"Oye Pando"*) and talk; the organism listens, thinks and
  answers aloud, and you can steer, approve or cancel any agent by voice.
- **The chat** — a complete native Pando client: sessions, streaming Markdown, tool
  cards and diffs, permission and question dialogs, steering, plans, goals, models and
  slash commands.

Everything that can run locally does: speech-to-text (Whistle), text-to-speech (Piper via
sherpa-onnx) and the decision model (Jev / `tev1` through Pando's System One endpoint).

> **Status:** planning complete, implementation not started. The full backlog
> (11 milestones, 15 epics, 49 stories, 170 tasks, 12 specs) lives in gintrack project
> `PDA` under `docs/.pmngr/`.

## How it fits together

```
 mic ─▶ capture ─▶ pando-voice worker (Whistle STT) ─▶ decision layer (System One)
                                                         │
                         ┌───────────── command ─────────┤──────── conversation ─────┐
                         ▼                                                            ▼
               Pando API (cancel / approve / spawn …)                 Pando chat stream (SSE)
                         │                                                            │
                         └──────────▶ organism + chat UI (GPUI) ◀─────────────────────┘
                                                │
                     speaker ◀─ pando-voice worker (Piper TTS) ◀─ conclusion / reply
```

## Repository layout

| Path | Contents |
|---|---|
| `crates/` | Rust workspace (to be created in milestone M0): `organism`, `decide`, `pando-client`, `voice-proto`, `pando-stt`, `pando-tts`, `claude-bridge`, `app` |
| `design-system/` | `tokens.json` (source of truth), brand SVGs, `reference/organism.html` (open it in a browser to see the target behaviour) |
| `docs/` | Planning notes `01`–`06`, verified research in `docs/research/`, ADRs and spike reports (to come) |
| `docs/.pmngr/` | Backlog managed with gintrack (do not edit by hand) |

## Documentation

| Doc | Topic |
|---|---|
| [docs/01-strategy.md](docs/01-strategy.md) | Goals, stack, crate layout, milestones, risks |
| [docs/02-voice-pipeline.md](docs/02-voice-pipeline.md) | Capture → STT → TTS → playback, barge-in |
| [docs/03-decision-layer.md](docs/03-decision-layer.md) | Command vs. conversation, confidence policy |
| [docs/04-pando-api.md](docs/04-pando-api.md) | Pando endpoints the assistant uses |
| [docs/05-claude-code-interop.md](docs/05-claude-code-interop.md) | Claude Code sessions as cells |
| [docs/06-ui-mapping.md](docs/06-ui-mapping.md) | API events → organism states and cells |
| [docs/research/pando-api.md](docs/research/pando-api.md) | Verified Pando API reference, gaps G1–G17 |
| [docs/research/bitacora-gpui.md](docs/research/bitacora-gpui.md) | GPUI patterns and reusable code from Bitácora |
| [docs/research/organism-port.md](docs/research/organism-port.md) | Porting the organism to Rust + GPUI |
| [docs/research/voice-stack.md](docs/research/voice-stack.md) | STT, wake word, TTS, audio, hotkeys, model distribution |
| [design-system/README.md](design-system/README.md) | The "living organism" design system |

Where the research contradicts the planning notes, the research is newer and verified;
reconciling them is task `PDA-T-0016`.

## Roadmap

| Milestone | Outcome |
|---|---|
| M0 | Workspace, CI, GPUI shell, design tokens, risk spikes |
| M1 | Organism on screen at 60 fps, parity with the reference |
| M2 | Auto-connect to Pando; real agents become cells |
| M3 | Complete native chat client |
| M4 | Push-to-talk → local transcription |
| M5 | Decision layer: voice commands vs. conversation |
| M6 | Spoken conversation, TTS, barge-in |
| M7 | Mini window, global hotkey, tray, settings |
| M8 | Claude Code sessions as cells, messaging |
| M9 | Hands-free wake word, echo cancellation |
| M10 | Signed packages, model manager, auto-update, v1.0 |

## Getting started (once M0 lands)

```bash
script/install-linux-deps.sh          # Linux only
scripts/fetch-needle.sh               # Whistle engine for your platform
cargo run -p app                      # needs a running `pando serve`, `pando app` or `pando desktop`
cargo run -p app -- --demo            # organism demo without Pando or audio
```

## Contributing

Read [AGENTS.md](AGENTS.md) first: language rules, the gintrack task workflow, the
two-worktree limit and disk hygiene, and the engineering rules.

## License

The source code of this repository is licensed under the [MIT License](LICENSE).

Third-party components keep their own licences and are listed in the NOTICE file shipped
with every package (task `PDA-T-0151`):

| Component | Licence | Notes |
|---|---|---|
| GPUI / GPUI Kit (`gpui-pre`, `gpui-kit`) | Apache-2.0 | |
| Whistle weights and needle engine (Cactus Compute) | Apache-2.0 | engine is redistributed as a prebuilt static library |
| sherpa-onnx, cpal | Apache-2.0 | ONNX Runtime (bundled by sherpa-onnx) is MIT |
| sonora (echo cancellation) | BSD-3-Clause | |
| Piper voice `es_ES-davefx-medium` | CC0 | other voices may require attribution (CC-BY) |
| Space Grotesk, JetBrains Mono, Zen Kaku Gothic New | SIL OFL 1.1 | |
| espeak-ng (phonemizer used by Piper/Kokoro) | GPL-3.0-or-later | linked **only** into the separate `pando-tts` worker |

The voice workers are split (ADR 0007). `pando-tts` (Piper/Kokoro TTS with espeak-ng) is
distributed as a GPL-3.0-or-later combined work, and every release ships its corresponding
source. `pando-stt` (Whistle STT, VAD, wake word) and the `pando-assistant` app are separate
processes with permissive licences (MIT and compatible) and contain no GPL code, enforced
with `cargo deny`. The decision is tracked in `PDA-T-0023`.

The "Pando" name and logos are covered by the MIT licence as files, but this does not
grant trademark rights.
