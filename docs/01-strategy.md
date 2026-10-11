# 01 · Project strategy

## Goal

A native desktop window — and a small always-on-top companion window — where the user
**talks** to mesnada (Pando's orchestrator) and **sees and controls** every agent it runs,
as one living organism. Everything that can run locally runs locally: speech-to-text,
text-to-speech and the decision model. No Python, no model wrappers: Rust binaries only.

Non-goals for v1: replacing the Pando TUI/WebUI for editing or browsing code; chat history
browsing; multi-user.

## Stack (decided)

| Concern | Choice | Why |
|---|---|---|
| UI | **GPUI** + `gpui-kit` (same as Bitácora) | Already known; immediate-mode canvas for the organism |
| Audio I/O | `cpal` (capture + playback), `rubato` (resample to 16 kHz mono) | Pure Rust, cross-platform |
| STT | **Whistle** (Cactus) via its C engine `libneedle.a` + `needle.h` through `bindgen` | 16.9 MB, 7 languages incl. Spanish, keyword biasing, 11 ms TTFT |
| TTS | **Piper** voices (`es_ES`) as ONNX via `ort` (or `sherpa-rs` as fallback) | Pure Rust runtime, good Spanish voices — **[verify quality]** |
| Decision | Pando's own `[DecisionModel]` (Ollama `tev1:0.8b` / TypeSafe Jev) over `POST /v1/systemone` | Pando already has the config, the router and the tested endpoint |
| Pando client | `reqwest` + SSE (`eventsource-stream`) | API is plain HTTP + SSE |
| Hotkeys | `global-hotkey` | ⌥ Space push-to-talk when unfocused |
| Echo cancel | `webrtc-audio-processing` (optional feature) | Barge-in without headphones |

## Workspace layout

```
pando-assistant/
├── Cargo.toml                 # workspace
├── crates/
│   ├── organism/              # pure simulation: Organism::step / ::shape — no GPUI
│   ├── voice-in/              # cpal capture, VAD/chunking, Whistle FFI (build.rs links libneedle)
│   ├── voice-out/             # Piper ONNX synthesis → cpal playback, RMS tap
│   ├── decide/                # typed questions → /v1/systemone; policies in code
│   ├── pando-client/          # typed API + SSE streams + conclusion parser
│   ├── claude-bridge/         # optional: Claude Code messaging socket, channel server, hook receiver
│   └── app/                   # GPUI app: main window, mini window, panels
├── design-system/             # tokens.json, README, reference/organism.html, brand/
├── docs/                      # these notes
└── vendor/needle/<target>/    # libneedle.a + needle.h per platform (not committed; fetched by script)
```

Rule: `organism` and `decide` have **no** UI or network dependency so they can be unit-tested
and fuzzed with recorded sessions.

## Milestones

1. **Organism on screen** — port `design-system/reference/organism.html` to `crates/organism`
   + a GPUI canvas. Fake agents, keyboard state switcher. Acceptance: matches the reference
   at 60 fps, hover/click/selection work.
2. **Live cells** — `pando-client` subscribes to `/api/v1/orchestrator/tasks` and
   `/api/v1/notifications/stream`; cells appear/die with real tasks; mitosis on spawn.
3. **Push-to-talk → text** — `voice-in` with Whistle; transcript shown; keyword list =
   project + agent names.
4. **Decision layer** — command vs. conversation; agent + action; project selection; low
   confidence asks the user. Commands hit the API directly (cancel/steer/respond), never the LLM.
5. **Conversation + TTS** — prompt to the active session via `/api/v1/chat/stream`; the final
   `<pando:conclusion>` block (or the last assistant message) is spoken; barge-in.
6. **Claude Code interop** — external sessions appear as cells (hooks); send/receive messages.
7. **Mini window**, global hotkey, settings, packaging (deb/AppImage, dmg, msi).

## Risks / open questions

- **Whistle licence** for the weights and the engine: check before distributing. Engine is
  distributed per target as a static lib; confirm a Linux x86_64 and aarch64 build exist.
- **Piper voice quality in Spanish** vs. Kokoro; both are ONNX — keep the TTS trait generic.
- **GPUI canvas performance** with ~2k path points/frame — if slow, rasterise with
  `tiny-skia` into an image and blit (the organism crate exposes a path list, so both work).
- Pando API **auth**: the API server uses a token (`/api/v1/config/api-server/regenerate-token`);
  the assistant must read it from Pando's config or ask once. **[verify]** where the WebUI
  reads it.
- The decision model is **off by default and fail-open** in Pando; the assistant should
  degrade gracefully: no decision model → everything is treated as conversation.

## Conventions

- Rust 2024 edition, `cargo clippy -D warnings`, `cargo fmt`.
- All async on `tokio`; audio threads are dedicated OS threads feeding channels.
- Feature flags: `echo-cancel`, `claude-bridge`, `tts-kokoro`.
- UI strings in `app/i18n/{es,en}.ftl` (fluent); Spanish is the source.
