# 01 · Project strategy

> Reconciled with the verified research in `research/` on 2026-10-12 (PDA-T-0016). Where
> this note and `research/*.md` disagree, the research wins; decisions are recorded as ADRs
> in [`adr/`](adr/README.md).

## Goal

A native desktop window — and a small always-on-top companion window — where the user
**talks** to mesnada (Pando's orchestrator) and **sees and controls** every agent it runs,
as one living organism. Everything that can run locally runs locally: speech-to-text,
text-to-speech and the decision model. No Python, no model wrappers: Rust binaries only.

Non-goals for v1: replacing the Pando TUI/WebUI for editing or browsing code; multi-user.
(The native chat client — sessions, history, tool cards — *is* in scope, milestone M3.)

## Stack (decided)

| Concern | Choice | Why |
|---|---|---|
| UI | **GPUI** via `gpui-kit = "=0.7.1"` (pins `gpui-pre 0.3.8`), only in `app`, behind `app/src/ui/` ([ADR 0001](adr/0001-gpui-kit-pin-and-ui-facade.md)) | Same as Bitácora; immediate-mode canvas for the organism |
| Audio I/O | `cpal` **0.18** (capture + playback, native PipeWire/Pulse hosts), `rubato` **5.x** FFT resampler to 16 kHz mono, `rtrb` rings | Pure Rust, cross-platform; audio stays in the app process for low-latency RMS |
| STT | **Whistle** (Cactus) via `libneedle.a` + `needle.h`, inside the **`pando-voice` worker process** ([ADR 0004](adr/0004-voice-worker-separate-process.md)) | 16.9 MB, 7 languages incl. Spanish, keyword biasing, streaming API |
| VAD / TTS | **sherpa-onnx** (single ONNX Runtime, [ADR 0005](adr/0005-sherpa-onnx-single-onnx-runtime.md)): Silero VAD, Piper `es_ES-davefx-medium` (CC0), Kokoro optional — in `pando-voice` | One runtime; espeak-ng (GPL-3) stays out of the app binary |
| Decision | Pando's `[DecisionModel]` config (Ollama `tev1:0.8b` / TypeSafe Jev); the assistant calls `{effectiveBaseURL}/v1/systemone` directly (03) | Pando has no decision proxy yet (gap G6, PDA-T-0165) |
| Pando client | `reqwest` (rustls, local CA) + own SSE parser, based on Bitácora's `pando-rs` core; REST + `POST /api/v1/chat/stream` ([ADR 0002](adr/0002-rest-chat-stream-not-ag-ui.md)) | Plain HTTP + SSE; AG-UI not used |
| Async | tokio behind the GPUI tokio bridge (`tokio::spawn` / `block_on` banned in `app`) | Bitácora pattern; avoids "no reactor running" panics |
| i18n | `rust-i18n` YAML locales, Spanish first ([ADR 0003](adr/0003-rust-i18n.md)) | Same library as GPUI Kit |
| Hotkeys | `global-hotkey` 0.8 (Windows, macOS, **X11 only**) + XDG GlobalShortcuts portal via `ashpd` on Wayland; CLI/IPC `ptt` fallback | No single crate covers Wayland |
| Echo cancel | `sonora` (pure-Rust WebRTC AEC3, feature `echo-cancel`); `webrtc-audio-processing` as fallback | Barge-in without headphones |

## Workspace layout

```
pando-assistant/
├── Cargo.toml                 # workspace (edition 2024, resolver 3)
├── crates/
│   ├── organism/              # pure simulation: step() / shape() → DrawList — no GPUI, no I/O
│   ├── decide/                # typed questions → /v1/systemone; policy in code; transport trait
│   ├── pando-client/          # Pando HTTP + SSE API, discovery, chat model (from pando-rs core)
│   ├── voice-proto/           # app ↔ pando-voice protocol
│   ├── voice-worker/          # bin `pando-voice`: Whistle STT, sherpa-onnx VAD/TTS (separate process)
│   ├── claude-bridge/         # Claude Code hooks receiver, messaging socket, channel
│   └── app/                   # bin `pando-assistant`: GPUI app (only crate that uses gpui-kit)
├── xtask/                     # tokens, check-deps, bundle, release
├── design-system/             # tokens.json, README, reference/organism.html, brand/
├── docs/                      # these notes, research/, adr/
└── vendor/needle/<target>/    # libneedle.a + needle.h per platform (not committed; fetched by script)
```

Rule: `organism` and `decide` have **no** UI, audio or network dependency so they can be
unit-tested and fuzzed with recorded sessions (`cargo xtask check-deps` enforces it).

## Milestones

The authoritative roadmap is the gintrack backlog (milestones M0–M10, see the root
`README.md`). In outline:

1. **Organism on screen** — port `design-system/reference/organism.html` to `crates/organism`
   + a GPUI canvas. Fake agents, keyboard state switcher. Acceptance: matches the reference
   at 60 fps, hover/click/selection work.
2. **Live cells** — `pando-client` polls `GET /api/v1/orchestrator/tasks` (≈1 s, diffed);
   cells appear/die with real tasks; mitosis on spawn. The notifications stream carries
   **no** task lifecycle; push events need upstream gap G1 (PDA-T-0155).
3. **Native chat** — sessions, streaming Markdown, tools, permissions, steering, `run_seq` reattach.
4. **Push-to-talk → text** — Whistle in `pando-voice`; transcript shown; keyword list =
   project + agent names.
5. **Decision layer** — command vs. conversation; agent + action; project selection; low
   confidence asks the user. Commands hit the API directly (cancel/steer/respond), never the LLM.
6. **Conversation + TTS** — prompt the active session via `POST /api/v1/chat/stream`; speak
   the reply accumulated from `content_delta` until `done` (there is no `response` event);
   delegated results arrive as `[delegated-result …]` text in history (04); barge-in.
7. **Mini window**, global hotkey, tray, settings.
8. **Claude Code interop** — external sessions appear as cells (hooks); send/receive messages.
9. **Hands-free** wake word and echo cancellation.
10. **Packaging** (deb/AppImage/Flatpak, signed dmg, signed msi), model manager, auto-update.

## Risks / open questions

- **Whistle engine is a closed-source prebuilt** released 2026-10-02: API/ABI churn. Pin HF
  commit SHAs, wrap behind `trait Stt`, keep the sherpa-onnx Parakeet fallback (PDA-T-0093).
  Licence: Apache-2.0 (weights and engine, see README § License).
- **No macOS x86_64 build** of libneedle → Intel Macs use the fallback STT.
- **Windows libneedle is GNU ABI (llvm-mingw)**, not linkable into the MSVC app → the
  `pando-voice` worker is built for `x86_64-pc-windows-gnullvm` (spike PDA-T-0019).
- **Linux libneedle needs libc++/libc++abi**, not libstdc++ (spike PDA-T-0018).
- **Whistle Spanish WER is unpublished** → measured in spike PDA-T-0018.
- **Piper vs. Kokoro Spanish quality** → spike PDA-T-0020; keep the TTS trait generic.
- **espeak-ng is GPL-3** → confined to the `pando-voice` worker ([ADR 0004](adr/0004-voice-worker-separate-process.md), PDA-T-0023).
- **GPUI canvas performance** with ~2k path points/frame → GPUI paths by default,
  `tiny-skia` raster fallback ([ADR 0006](adr/0006-tiny-skia-tests-and-fallback-only.md), spike PDA-T-0024).
- **Pando API auth (resolved):** the API token is **ephemeral** (regenerated at each server
  start, not persisted). Get it from `GET /api/v1/token` (no credentials on a loopback bind)
  and send it as the `X-Pando-Token` header; refetch on 401. Never call
  `regenerate-token`. `pando serve`/`app` are **https** with the CA at
  `~/.config/pando/tls/ca.crt`; `pando desktop` is plain http.
- **Pando API gaps** (G1–G17 in `research/pando-api.md` §6.2) are upstream tasks
  PDA-T-0155…PDA-T-0170; the assistant degrades gracefully until they land.
- The decision model is **off by default and fail-open** in Pando; the assistant does the
  same: no decision model → everything is treated as conversation.
- **Wayland**: no always-on-top for normal windows and no global hotkeys without portals
  (spikes PDA-T-0025, PDA-T-0026).

## Conventions

- Rust 2024 edition, toolchain pinned in `rust-toolchain.toml`, `cargo clippy -D warnings`, `cargo fmt`.
- Async on tokio through the GPUI tokio bridge; audio callbacks never allocate or lock and
  feed `rtrb` rings drained by worker threads.
- Feature flags: `echo-cancel`, `claude-bridge`, `tts-kokoro`, `raster`.
- UI strings in `crates/app/assets/locales/*.{es,en}.yml` (`rust-i18n`); Spanish is the source.
