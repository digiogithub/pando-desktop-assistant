# Spike S3 · One sherpa-onnx: Silero VAD + Piper + Kokoro, alongside libneedle

- **Task:** PDA-T-0020 (story PDA-US-0006) · **Date:** 2026-10-12 · **Status:** Linux x86_64 done; other targets pending
- **Code:** [`spikes/s3-sherpa-tts/`](../../spikes/s3-sherpa-tts/) (standalone crate, excluded from the workspace)
- **Builds on:** [`docs/research/voice-stack.md`](../research/voice-stack.md) §1.3, §2.3, §3; [ADR 0004](../adr/0004-voice-worker-separate-process.md); [ADR 0005](../adr/0005-sherpa-onnx-single-onnx-runtime.md)

## TL;DR

| Question | Answer |
|---|---|
| Does one statically linked `sherpa-onnx` 1.13.8 run Silero VAD + Piper + Kokoro in one process? | **Yes.** One ORT, 605 MB RSS with all three loaded (Kokoro fp32 is most of it). |
| Time to first audio, 10-word Spanish sentence, Piper `es_ES-davefx-medium` | **104–132 ms warm** (4/2 threads), 125–145 ms cold. **Meets < 300 ms.** |
| Same with Kokoro (`ef_dora`) | fp32: **~500 ms** for a whole 10-word sentence (fails); 260 ms when the first chunk is a short sentence. int8: 1.1–1.3 s (worse than fp32 on this CPU). |
| Silero VAD v5 cost / latency | **~53 µs per 32 ms window** (p95 59 µs, ~0.17 % of a core); onset reported 150–175 ms after true onset with `min_speech_duration` 0.1 s. |
| libneedle in the **same binary** as sherpa-onnx | **Links, but breaks C++ exceptions** in the libstdc++ code (sherpa/ORT) unless libneedle's libc++ is made symbol-private. With the recipe below, everything works (Whistle STT + Piper + Kokoro + concurrency). |
| Windows | **sherpa-onnx's prebuilt static libs are MSVC (`win-x64-static-MT`)**, libneedle is GNU ABI (llvm-mingw): they **cannot share a binary on Windows** without building sherpa-onnx from source with llvm-mingw. |
| Licence | Even a **VAD-only** binary statically pulls **espeak-ng** (180 `espeak_*` symbols), because the C API object references the TTS code. See S7 / ADR 0007. |

**Decision for implementation:** default voice **Piper `es_ES-davefx-medium` fp32, 2 threads**
(first audio ~130 ms, RTF 0.04). Kokoro stays optional (fp32 only, 4 threads, short first
chunk; int8 is slower here). Never link sherpa-onnx into the `app` binary. Whether
libneedle and sherpa-onnx share a process is decided in ADR 0007 (licence) — technically it is
possible on Linux only with the symbol-private libneedle recipe.

## Environment

Intel Core Ultra 9 285 (24 cores, P+E), 93 GB RAM, Pop!_OS 24.04 (kernel 7.1), Rust 1.98.1,
`sherpa-onnx = "=1.13.8"` with default `static` feature (downloads
`sherpa-onnx-v1.13.8-linux-x64-static-lib.tar.bz2` from the k2-fsa release). Models under
`~/.cache/pando-assistant-spikes/` (never committed):

| Model | Size | SHA-256 (as shipped by k2-fsa `tts-models` / `asr-models` releases) |
|---|---|---|
| `vits-piper-es_ES-davefx-medium` (dir) | 79 MB (`.onnx` 63.1 MB) | `6826f9ac…6163eb` (sherpa adds metadata, so it differs from the rhasspy file `6658b03b…`) |
| `vits-piper-es_ES-davefx-medium-int8` | 37 MB | — |
| `kokoro-multi-lang-v1_0` (54 voices, `ef_dora`=28, `em_alex`=29) | 384 MB (`model.onnx` 325.6 MB, `voices.bin` 28 MB) | `b40f62b1…05da11` |
| `kokoro-int8-multi-lang-v1_0` | 183 MB (`model.int8.onnx` 114 MB) | `4b86207e…6bb36` |
| `silero_vad_v5.onnx` | 2.3 MB | `6b99cbfd39246b6706f98ec13c7c50c6b299181f2474fa05cbc8046acc274396` |
| `silero_vad.onnx` (v4) | 0.6 MB | `9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6` |

`kokoro-multi-lang-v1_1` is a zh/en model and has no Spanish voices: use **v1_0**.

## Measurements (Linux x86_64)

`s3-sherpa-tts piper|kokoro …` uses `max_num_sentences = 1`, so sherpa's progress callback
fires once per sentence and its first call is "first audio". Warm = median of 5 after one
warm-up. Text: *"La tarea de migración auth ha terminado sin errores hoy."* (10 words);
paragraph = 4 sentences, ~10.5 s of speech.

| Engine | Threads | Load | Cold first audio | Warm first audio (10 words) | RTF | Paragraph first audio |
|---|---|---|---|---|---|---|
| Piper davefx fp32 | 1 | 570 ms | 354 ms | 326 ms (min 207, max 341) | 0.104 | 94 ms |
| Piper davefx fp32 | **2** | 411 ms | 145 ms | **132 ms** (130–140) | **0.042** | 64 ms |
| Piper davefx fp32 | 4 | 491 ms | 125 ms | 104 ms (100–121) | 0.033 | 47 ms |
| Piper davefx int8 | 2 | 823 ms | 359 ms | 333 ms | 0.103 | 153 ms |
| Kokoro int8 `ef_dora` | 2 | 848 ms | 1356 ms | 1297 ms | 0.403 | 704 ms |
| Kokoro int8 `ef_dora` | 4 | 759 ms | 1143 ms | 1128 ms | 0.351 | 566 ms |
| Kokoro fp32 `ef_dora` | 4 | 908 ms | 523 ms | 508 ms | 0.157 | 259 ms |
| Kokoro fp32 `em_alex` | 4 | — | — | 493 ms | 0.156 | 263 ms |

Notes:
- Single-thread numbers vary a lot (207–341 ms) — on this hybrid CPU the thread lands on a P or an
  E core. Use **2 threads**; it is stable and leaves cores for STT.
- int8 models are *slower* than fp32 on this x86 CPU (dynamic-quant kernels); they only save disk.
- Kokoro's first audio is bounded by the first sentence: keep the first chunk short (cut at a
  comma above ~12 words, as `voice-stack.md` §3.2 already says) and it gets to ~260 ms with fp32.

**Silero VAD** (`s3-sherpa-tts vad`): clip = 1 s silence + Piper speech + 1 s silence at 16 kHz,
`threshold 0.5, min_silence 0.25 s, min_speech 0.1 s, window 512`.

| Model | Load | Per 32 ms window p50 / p95 / max | Onset detected after true onset | Segment closed after true end |
|---|---|---|---|---|
| v5 | 25 ms | 53 / 59 / 180 µs | 174 ms | 346 ms |
| v4 | 4 ms | 51 / 61 / 178 µs | 149 ms | 335 ms |

The VAD segment itself starts at the right place (1.01 s vs true 1.04 s, sherpa keeps
pre-roll); the "detected" flag lags by `min_speech_duration` plus one or two windows. End-of-turn
lag is `min_silence_duration` + ~100 ms. Keep v5 (more robust in noise per upstream).

**All in one process** (`s3-sherpa-tts all`): VAD v5 + Piper (2 thr) + Kokoro int8 (2 thr)
created together, then exercised: identical numbers to the separate runs, max RSS **605 MB**,
no crash. One ORT (sherpa's bundled one) serves all three.

### Spanish quality (informal)

A real MOS needs human listeners: **pending, owner to listen** to the samples in
`~/.cache/pando-assistant-spikes/s3-samples/` (`piper_t4.wav`, `piper_int8.wav`,
`kokoro_fp32_dora.wav`, `kokoro_fp32_alex.wav`, `kokoro_int8_dora_t4.wav`; same paragraph).

As an objective proxy, `s3-combo` synthesises five product phrases and transcribes them with
Whistle (`language=es`, keywords `Pando, bitácora, auth`). Round-trip WER (TTS + STT errors
together):

| Voice | Round-trip WER | Typical errors |
|---|---|---|
| Piper davefx | 20.5 % (9/44) | "auth" → "usted", "pull request" → "pública que está", "el agente" → "en la gente" |
| Kokoro `ef_dora` | 15.9 % (7/44) | "pull request" → "púrre que está", "Oye" → "Olle" |
| Kokoro `em_alex` | 15.9 % (7/44) | same pattern |

Almost all errors are **English loanwords** read with Spanish rules ("auth", "pull request") and
"Oye" → "Olle" (homophone). Pure Spanish sentences round-trip with 0 errors on every voice. This
confirms the text-normalisation step planned in `02-voice-pipeline.md` (map ids and English terms
to a pronounceable form before TTS) matters more than the choice of voice.

## libneedle in the same binary

libneedle (`linux-x86_64/libneedle.a`, clang + **libc++ 21**) was available from the S1 spike
(`~/.cache/pando-assistant-spikes/needle/`, LLVM 21 `libc++.a`/`libc++abi.a` extracted from the
Debian packages). sherpa-onnx's static archive is built with GCC and links **libstdc++**
dynamically (`ldd`: `libstdc++.so.6`, `libgcc_s.so.1`).

### What breaks

`s3-ehtest` links a 10-line g++ (libstdc++) shim that throws and catches
`std::out_of_range`/`std::runtime_error` 2000 times:

| Link variant | Exported `__cxa_throw`/`__gxx_personality_v0` from the exe | Shim result |
|---|---|---|
| Shim only (baseline) | 0 | 2000/2000 caught |
| + `libneedle.a` + `libc++.a` + `libc++abi.a` | 2 | **abort: "Rust cannot catch foreign exceptions"** |
| + `libneedle.a` + `libc++.a` only | 2 (Debian's `libc++.a` bundles the libc++abi objects) | **abort** |
| + **symbol-private libneedle** (recipe below) | 0 | **2000/2000 caught** |

Cause: the static libc++abi defines `__cxa_throw`, `__gxx_personality_v0`, `__cxa_begin_catch`…
in the executable, the linker exports them, and they **interpose libstdc++'s** at run time. Every
C++ exception thrown by libstdc++-built code (onnxruntime, sherpa-onnx, espeak-ng glue) then goes
through libc++abi and is not caught by its own handler, so it unwinds into Rust and aborts. ORT
uses exceptions for ordinary error paths (bad model, shape mismatch), so the naive link is a
crash waiting to happen. This is the R4 risk of `voice-stack.md`, now confirmed.

### Recipe: make libneedle's libc++ private

```bash
L=<llvm-21>/lib            # libc++.a (+ libc++abi objects on Debian/Ubuntu)
ld -r -o needle_all.o --whole-archive libneedle.a --no-whole-archive $L/libc++.a
nm needle_all.o | awk '$2=="T" && $3 ~ /^needle_/ {print $3}' > keep.txt     # 11 symbols
# The personality routine is referenced through a COMDAT symbol (DW.ref.*); rename it so it
# can stay global without merging with libstdc++'s, otherwise lld fails with
# "relocation refers to a symbol in a discarded section".
objcopy --redefine-sym __gxx_personality_v0=__pnd_needle_gxx_personality_v0 \
        --redefine-sym DW.ref.__gxx_personality_v0=DW.ref.__pnd_needle_gxx_personality_v0 \
        needle_all.o needle_ren.o
echo DW.ref.__pnd_needle_gxx_personality_v0 >> keep.txt
objcopy --keep-global-symbols=keep.txt needle_ren.o needle_priv.o
ar rcs libneedle.a needle_priv.o     # only needle_* (+ the renamed DW.ref) are global
```

The result needs only glibc (`__cxa_atexit`, `__cxa_thread_atexit_impl` stay undefined) and no
`libc++.so` at run time. Build the spike with
`NEEDLE_DIR=<dir of that libneedle.a> S3_NEEDLE_PRIVATE=1 cargo build --release --features needle --target-dir ../../target/spikes`.

### Result with the private libneedle (`s3-combo`)

- `needle_load(junk)` → `-1 ("invalid .cact model")`, clean; `needle_load(whistle.cact)` → 0 in 12 ms.
- TTS → STT round trip for 3 voices × 5 phrases: Whistle 106–183 ms per phrase (table above).
- **Concurrency**: Whistle transcribing on one thread (5×: 153–215 ms) while Piper synthesises on
  another (10×: 112–146 ms): no crash, no deadlock, wall 1.25 s. Max RSS 805 MB (Whistle +
  Piper + Kokoro fp32).
- Note: libneedle imports `sched_setaffinity`/`sched_getcpu`; it may pin its threads. Watch for
  contention with ORT's pool in the real worker (no slowdown seen here).

### Other findings

- **A corrupt model aborts the process.** `OfflineTts::create` with a garbage `.onnx` lets an ORT
  C++ exception escape the C API (`fatal runtime error: Rust cannot catch foreign exceptions`),
  even with no libneedle around. The worker must **verify the SHA-256 of every model before
  loading it**; the crash isolation of ADR 0004 is what keeps the UI alive if this ever happens.
- Binary size: 36.6 MB (30.0 MB stripped) for sherpa-onnx + ORT + espeak-ng, before models.
- **Licence:** `s3-vadonly` (only `VoiceActivityDetector`) still contains 180 `espeak_*`
  symbols. Any binary that links the prebuilt sherpa-onnx static libs contains espeak-ng
  (GPL-3.0-or-later). Getting a VAD without espeak-ng requires building sherpa-onnx from
  source with `-DSHERPA_ONNX_ENABLE_TTS=OFF`. Feeds the S7 decision ([ADR 0007](../adr/0007-espeak-ng-gpl-isolated-tts-worker.md)).

## Symbol-clash check to repeat on every target

Run on the final worker binary (and in CI once packaging exists):

```bash
B=target/release/pando-voice          # or the spike binary
ldd $B                                # expect libstdc++ + libgcc_s, no libc++.so
nm -D $B | grep -E ' T (__cxa_throw|__gxx_personality_v0|__cxa_begin_catch)$'   # expect nothing
nm $B | grep -c 'St3__1'              # libc++ symbols present only as local (t/d), never T/W
nm -g --defined-only $B | grep -E 'St3__1|__cxa_|__gxx_' # expect nothing
nm $B | grep -ci 'OrtGetApiBase'      # exactly one ORT (2 lines: symbol + its alias)
nm $B | grep -c ' espeak_'            # >0 means the binary is GPL-3 (see ADR 0007)
./s3-ehtest                           # 2000/2000 caught, no abort
```

## Pending (not doable on this box)

- **Linux aarch64, macOS arm64, Windows**: no cross toolchains here. sherpa-onnx ships static libs
  for linux-aarch64, osx-arm64 and win-x64 (MSVC MT); repeat the table and the symbol check on
  each. On macOS both sides use the system libc++, so the clash above should not exist (verify).
- **Windows + libneedle in one binary is not possible with prebuilt libs** (MSVC vs GNU ABI).
  Needs either sherpa-onnx built with llvm-mingw for `x86_64-pc-windows-gnullvm`, or STT and
  TTS in different processes (ADR 0007 proposes the latter for licence reasons anyway).
- **Informal MOS** for Piper vs Kokoro in Spanish: needs a human listener (samples above).
