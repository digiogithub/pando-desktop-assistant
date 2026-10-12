# 0004 · Voice (STT, VAD, TTS) runs in a separate `pando-voice` worker process

- **Status:** accepted; partially superseded by 0007
- **Date:** 2026-10-12
- **Deciders:** project owner, planning agents
- **Refs:** PDA-T-0019, PDA-T-0023, PDA-T-0087, PDA-T-0090, PDA-T-0091, [`docs/research/voice-stack.md`](../research/voice-stack.md) §1.3–1.4, §3.3, §8

## Context

- Whistle's engine (`libneedle.a`) is a closed-source C++ static library, process-global
  and not thread-safe (one speech model per process).
- On Linux it is built with clang + libc++, not libstdc++; on Windows it is an llvm-mingw
  (GNU ABI) archive that **cannot be linked into the MSVC binary** GPUI builds on.
- Piper/Kokoro through sherpa-onnx link espeak-ng, which is **GPL-3.0-or-later**; the app
  binary must stay MIT-compatible.
- A crash inside a one-week-old C++ engine must not take down the UI.

## Decision

All voice inference runs in a separate executable, `pando-voice` (crate `voice-worker`),
spawned and supervised by the app. The protocol lives in `voice-proto` (length-prefixed
frames over stdio/pipe: PCM in, JSON/PCM out). On Windows the worker is built for
`x86_64-pc-windows-gnullvm`; elsewhere for the native target. Audio capture and playback
stay in the app (cpal) so the organism gets RMS without IPC latency.

## Consequences

- Same code path on every OS; Windows ABI problem solved.
- Crash isolation: the supervisor restarts the worker and the UI degrades to text.
- The worker is a GPL-3 combined work (espeak-ng); our code inside it stays MIT and
  releases ship its corresponding source. The app binary contains no GPL code.
- Cost: ~1 ms IPC per call, one more binary to package and sign.

## Alternatives considered

| Option | Why not |
|---|---|
| In-process STT/TTS | Cannot link on Windows MSVC; GPL in the app binary; no crash isolation |
| Whole app on `*-windows-gnullvm` | GPUI support unknown |
| Own Spanish G2P (no espeak) | Viable later hardening (PDA-T-0023), not needed for v1 |
