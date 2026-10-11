# Spike S2 · `pando-stt` sidecar on Windows (`x86_64-pc-windows-gnullvm`)

- **Task:** PDA-T-0019 (story PDA-US-0006) · **Date:** 2026-10-12 · **Author:** claude-opus
- **Code:** [`spikes/whistle-ffi/`](../../spikes/whistle-ffi/) (`src/bin/pando-stt.rs`, `src/bin/stt-host.rs`, `build.rs`)
- **Related:** [ADR 0004](../adr/0004-voice-worker-separate-process.md), [S1](S1-whistle.md), `docs/research/voice-stack.md` §1.3, risk R3

## Question

Can a Rust binary built for `x86_64-pc-windows-gnullvm` link the llvm-mingw (GNU ABI, UCRT,
libc++) `libneedle.a`, and can an MSVC GPUI host drive it over stdio (length-prefixed PCM in,
JSON out) with less than 20 ms of IPC overhead for a 5 s clip?

## Method

1. Cross-compiled on Linux (no Windows machine available to the agent):
   - Rust 1.98.1 + `rustup target add x86_64-pc-windows-gnullvm`.
   - Linker and C++ runtime from **llvm-mingw 20261006 (UCRT, clang 23.1.3)**:
     `CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER=x86_64-w64-mingw32-clang`,
     static `libc++.a`, `libc++abi.a`, `libunwind.a` from `x86_64-w64-mingw32/lib`.
   - `libneedle.a` windows-x86_64 pinned at needle3 commit `93e88ede…`, SHA-256
     `ef057b31…e425672` (verified).
2. Protocol (spike version): request = `u32 LE n_samples` + `n_samples × f32 LE` (16 kHz mono);
   `n_samples = 0` ends the session. Response = `u32 LE len` + UTF-8 JSON (engine JSON plus
   `engine_ms` measured inside the sidecar). `--echo` skips the engine to isolate pure IPC.
3. `stt-host` spawns the sidecar, sends the same 5 s FLEURS clip (320 004 bytes per frame) N
   times, drops the warm-up round and reports round-trip (RTT), engine time and
   **IPC overhead = RTT − engine_ms** (P50/P95/max).
4. Ran the Windows `.exe` under **Wine 11.0** on the dev box (Intel Core Ultra 9 285, 24 cores),
   with both a native Linux host and a Wine (gnullvm) host, plus a native Linux baseline.

Reproduce:

```bash
LM=~/.cache/pando-assistant-spikes/llvm/llvm-mingw-20261006-ucrt-ubuntu-22.04-x86_64
PATH=$LM/bin:$PATH CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER=x86_64-w64-mingw32-clang \
LIBCXX_DIR=$LM/x86_64-w64-mingw32/lib \
cargo build --release --target x86_64-pc-windows-gnullvm --target-dir ../../target/spikes
stt-host <pando-stt[.exe]> clip5s.wav 21 -- whistle.cact --lang es
```

## Results

**Link: works.** `pando-stt.exe` (6.2 MB, release, line tables) links with no unresolved
symbols. Its import table holds only `KERNEL32.dll`, `ntdll.dll`, `api-ms-win-core-synch` and
the `api-ms-win-crt-*` UCRT forwarders: **no `libc++.dll`, `libunwind.dll` or MinGW runtime
DLLs** to ship. The binary transcribes correctly under Wine (same text as the Linux build:
"A causa de la topología submarina,").

| Setup (5 s clip, 320 KB frame) | Rounds | RTT P50 | Engine P50 | IPC overhead P50 | P95 | Max |
|---|---|---|---|---|---|---|
| Linux host → Linux sidecar, `--echo` | 50 | 0.08 ms | — | 0.08 ms | 0.24 ms | 0.32 ms |
| Linux host → Linux sidecar, Whistle | 20 | 71.3 ms | 71.1 ms | 0.20 ms | 2.08 ms | 3.01 ms |
| Linux host → **Wine `pando-stt.exe`**, `--echo` | 50 | 0.08 ms | — | 0.08 ms | 0.15 ms | 0.22 ms |
| Linux host → **Wine `pando-stt.exe`**, Whistle | 20 | 68.9 ms | 67.8 ms | 0.82 ms | 2.33 ms | 2.93 ms |
| **Wine `stt-host.exe` → Wine `pando-stt.exe`**, Whistle | 20 | 83.9 ms | 82.9 ms | 1.02 ms | 1.76 ms | 1.96 ms |

Cold start (spawn + 17 MB model load + first 5 s transcription): 136–225 ms (Wine), 139 ms
native. Whistle under Wine runs at native speed (same x86 code, no syscall-heavy path).

**Acceptance (< 20 ms IPC overhead for a 5 s clip): met with a ~7× margin** on every setup
measured (worst P95 2.3 ms, worst max 3.0 ms). The overhead above the pure-copy floor
(~0.1 ms) is scheduler wake-up of the engine's worker threads, not the pipe.

## What could not be verified here (pending, needs a Windows machine)

1. **A real MSVC host.** Building an `x86_64-pc-windows-msvc` host here needs the MSVC CRT/SDK
   (`cargo-xwin` downloads them under Microsoft's licence terms; not accepted on the user's
   behalf). The protocol is plain bytes over anonymous pipes, so the host's ABI does not
   enter the contract; the Wine `stt-host.exe` run already exercises Windows pipe semantics
   through Wine's emulation. Still to run on Windows 10/11: MSVC GPUI host → gnullvm sidecar.
2. **Native Windows timings** (real NT pipes, Defender real-time scanning of a freshly
   written `.exe`, cold start from disk), on the reference laptop.
3. **Process flags:** spawn with `CREATE_NO_WINDOW` so no console flashes; Job Object so the
   sidecar dies with the app; check SmartScreen/Authenticode signing of the second binary.
4. **windows-arm64:** needle ships `windows-arm64`; `aarch64-pc-windows-gnullvm` + llvm-mingw
   should work the same way but was not built.

## Decision / recommendation

- **Go.** Confirms [ADR 0004](../adr/0004-voice-worker-separate-process.md) (worker process,
  gnullvm on Windows); no new ADR needed.
- Build recipe for the `voice-worker` crate on Windows: target `x86_64-pc-windows-gnullvm`,
  llvm-mingw UCRT toolchain as linker, `static=c++`, `static=c++abi`, `static=unwind`. Pin the
  llvm-mingw release (and its SHA-256) in `xtask` like the needle artefacts.
- The production `voice-proto` needs more than this spike's PCM-only frame: a typed header
  (request id, kind = transcribe / stream chunk / stream stop / config / shutdown), language and
  keyword list per request or as a config message, and an error frame. Keep payloads as raw
  `f32 LE` (a 5 s clip is 320 KB; copying it costs ~0.1 ms).
- Stream PCM in ~1 s chunks during PTT (`needle_stream_transcribe_process`) so the final
  transcription after key-up only processes the tail.

## Follow-ups

- Run the pending checks above on Windows hardware (keep PDA-T-0019 in review until done).
- `voice-worker` crate task: add the gnullvm cross-build to `xtask` (no CI for now).
