---
created_at: 2026-10-12T07:55:53.471074287Z
updated_at: 2026-10-12T07:55:53.471074287Z
tags:
    - changes
    - voice
---
# Voice workers implementation (ADR 0007 split)

Plan: [[pando-assistant/plans/backlog-execution_plan.md]]. Builds on [[pando-assistant/changes/accept-adr-0007-0008]], [[pando-assistant/changes/voice-spikes-s1-s2-s5]], spikes S1/S3/S4 in `docs/spikes/`.

## What changed
- `crates/voice-worker` split into `pando-stt` (Whistle/libneedle, never prebuilt sherpa-onnx) and `pando-tts` (sherpa-onnx Piper/Kokoro, GPL as distributed binary). `xtask check-deps`: new edges, sherpa-onnx forbidden outside the two workers, `pando-stt` may reach it only when `SHERPA_ONNX_LIB_DIR` is set (source build, TTS off). Refs PDA-T-0023.
- `voice-proto` (PDA-T-0087): postcard in u32-LE frames, `PROTOCOL_VERSION=1`, `ToWorker`/`FromWorker`, `FrameDecoder`, fuzz test.
- `needle-sys` (PDA-T-0088): 11 bindings, `build.rs` (NEEDLE_DIR, vendor/needle/<plat>, libc++ search, `libneedle_private.a` preferred, NEEDLE_ALLOW_MISSING), `scripts/fetch-needle.sh` (pinned HF SHAs + SHA-256), `scripts/privatize-needle.sh` (S3 recipe).
- `pando-stt` (PDA-T-0089/0092/0093): `Stt` trait, `Whistle`, <=5 s pause-aware chunking, `KeywordList` + `resolve_language`, engine routing `engine::select`, feature `parakeet` (off), stdio worker.
- `audio` (PDA-T-0084/0109): `Capture` (16 kHz mono 20 ms frames, pre-roll, level, watchdog rebuild) and `Playback` (ring, atomic stop, output RMS, events).
- `speech` (PDA-T-0108/0110/0111): `normalize` (es/en), `segment`, `SpeechQueue`.
- `pando-tts` (PDA-T-0107): `Tts`, `SherpaTts`, `synthesize_stream`, protocol server with cancel.

## Files and symbols
`crates/{voice-proto,needle-sys,pando-stt,pando-tts,audio,speech}`, `scripts/{fetch-needle,privatize-needle,build-sherpa-stt}.sh`, `xtask/src/deps.rs` (`check_with`, `is_sherpa`).

## Why
Backlog tasks above; ADR 0007 (licence isolation) and ADR 0008 (Whistle default, <=5 s calls, keyword bias).

## Verification
- `cargo test`/clippy -D warnings per crate, `cargo xtask check-deps` OK (12 crates). `nm`: 0 `espeak_` symbols in pando-stt, 53 in pando-tts.
- Live (#[ignore]): Whistle transcribes the Spanish fixture and chunks >5 s; keyword recall 82.9 % (34/41) on synthetic commands (target 90 %, needs real voices); Piper first PCM chunk 113-115 ms; Kokoro ef_dora works; playback stop 6-29 ms, PipeWire stream start 6 ms (Pulse host ~2 s).
- Parakeet works against upstream prebuilt sherpa-onnx (dev only). A local source build with g++ 12/13 crashes inside ORT std::regex (ORT archive is GCC 11): build in a GCC 11 container.

## Findings worth keeping
- libneedle (static libc++) must be symbol-private to live in the same binary as libstdc++ code (sherpa/ORT): [[pando-assistant/changes/voice-workers-implementation|privatize-needle.sh]].
- cpal Pulse host `Stream::play` takes ~2 s; enable the `pipewire` feature of `audio` on Linux.
- The app must wire: supervisor PDA-T-0090 (spawn `pando-stt`/`pando-tts`, handshake, restart), `audio` capture -> STT, TTS Pcm -> `Playback::play`, `SpeechQueue`, `normalize` before `Synth`.
