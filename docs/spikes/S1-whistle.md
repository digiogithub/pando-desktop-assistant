# Spike S1 · Whistle FFI + Spanish WER

- **Task:** PDA-T-0018 (story PDA-US-0006) · **Date:** 2026-10-12 · **Author:** claude-opus
- **Code:** [`spikes/whistle-ffi/`](../../spikes/whistle-ffi/): `src/sys.rs` (hand-written
  `needle.h` bindings), `src/lib.rs` (safe `Whistle` wrapper), `build.rs` (static libc++),
  `src/bin/bench.rs`, `src/bin/stream.rs`, `scripts/` (data prep, sherpa baselines, WER)
- **Risks:** R1, R4, R5 of `docs/research/voice-stack.md` §8 · **Decision:** [ADR 0008](../adr/0008-stt-engines-whistle-default-parakeet-optional.md)
- **Related:** [S2](S2-windows-stt-sidecar.md), [S3](S3-sherpa-onnx-single-runtime.md) (libneedle next to sherpa-onnx), [S5](S5-wake-word.md)

## 1. FFI and linking (linux-x86_64)

- Artefacts pinned and SHA-256-verified against `voice-stack.md` §6.2: `libneedle.a`
  linux-x86_64 `5f8ed375…04e5cd`, `whistle.cact` `b6e02f04…1dffeb` (needle3 `93e88ede…`, whistle `ca528760…`).
- **Hand-written bindings are enough.** `needle.h` has 11 functions with C types only, so
  bindgen/libclang is not needed at build time (`src/sys.rs`, about 40 lines).
- **Static libc++ works with the default GNU toolchain (gcc as linker, no clang installed).**
  `build.rs` links `static=needle`, `static=c++`, `static=c++abi` (LLVM **21.1.8**, from the
  apt.llvm.org `libc++-21-dev`/`libc++abi-21-dev` debs, matching the `ne210100` ABI tag) plus
  `pthread`, `m`, `dl`. The release `bench` is 6.1 MB and its `ldd` shows only libc, libm,
  libgcc_s and ld-linux: **no `libc++.so.1` runtime dependency.** The unwinder is libgcc_s,
  shared with Rust.
- Safe wrapper: `Whistle` is `!Send + !Sync` (`PhantomData<*const ()>`), there is one per
  process (atomic guard), and it keeps a 256 KiB output buffer. The model loads in **12–13 ms**
  from memory (17 MB).
- **Caveat (R4), measured by S3:** in a binary that *also* contains libstdc++ code
  (sherpa-onnx/ORT), the static libc++abi takes over `__cxa_throw`/personality and breaks C++
  exceptions unless libneedle's libc++ is made symbol-private (recipe in S3). In the separate
  `pando-stt` worker of [ADR 0004](../adr/0004-voice-worker-separate-process.md) and
  [ADR 0007](../adr/0007-espeak-ng-gpl-isolated-tts-worker.md) this does not arise. Keep
  libneedle out of any process that loads sherpa-onnx.
- **macos-arm64: not run** (no Mac available to the agent). Pending, see §6.

## 2. Method

| Set | Source (licence) | Clips | Notes |
|---|---|---|---|
| `fleurs` | FLEURS `es_419` test (CC-BY-4.0, `google/fleurs`) | 200 random (seed 42) | read Wikipedia sentences, 16 kHz |
| `mls` | Multilingual LibriSpeech Spanish test (CC-BY-4.0, `facebook/multilingual_librispeech`) | 200 random | audiobooks, opus → 16 kHz; **stands in for Common Voice** (see below) |
| `cmd` | 50 own assistant commands with project names (`scripts/commands_es.txt`) × 3 Piper voices (`es_ES-davefx` CC0, `es_ES-sharvard` CC-BY-3.0, `es_MX-ald` Unlicense) | 150 | **synthetic** (sherpa-onnx TTS) |
| `cmdnoisy` | `cmd` + pink noise at 10 dB SNR | 150 | |
| `lat3` / `lat10` | FLEURS clips cut to exactly 3 s / 10 s | 30 each | latency only |

- **Common Voice was not used.** Since 2025 it is distributed through the Mozilla Data
  Collective, which needs an account and acceptance of its terms; the agent did not accept
  them for the user. MLS Spanish (a different speaker population, also CC-BY-4.0) is used
  instead. Adding CV es later only needs a new `list_/refs_` pair.
- Engines: **Whistle** through the Rust FFI (`bench`), `language=es` unless noted, optional
  keyword bias `scripts/keywords_es.txt` (21 project terms). **Parakeet-TDT-0.6b-v3 int8**
  and **Whisper small int8** through sherpa-onnx 1.13.8 (Python wheel, `num_threads=4`;
  Whisper with `language=es`). Whistle picks its own thread count (≤ 4 fast cores).
- WER (`scripts/wer.py`, jiwer): lowercase, punctuation removed, accents kept. "no-acc" also
  strips accents. Numbers are not normalised (MLS spells them out).
  **Keyword recall** = share of the reference's project terms (`keywords_es.txt`) that appear
  in the hypothesis.
- Machine: Intel Core Ultra 9 285 (24 cores, x86_64), Pop!_OS 24.04. Other agents were
  building in parallel, so absolute timings carry some noise. The latency curves were
  re-checked and are consistent.

Reproduce: `scripts/prep_s1.py`, then `scripts/run_s1.sh` (models and data in
`~/.cache/pando-assistant-spikes/`, never committed).

## 3. Accuracy (WER %, lower is better)

| Engine | FLEURS es | MLS es | Commands | Commands + 10 dB noise | Keyword recall (cmd / noisy) |
|---|---|---|---|---|---|
| Whistle, auto language | — | — | 27.1 | 32.4 | 31.7 / 23.6 % |
| Whistle, `es` | 9.7 | 13.9 | 25.5 | 31.5 | 31.7 / 23.6 % |
| **Whistle, `es` + keyword bias** | — | — | **17.9** | **22.4** | **80.5 / 71.5 %** |
| **Parakeet v3** (sherpa) | **4.3** | **6.0** | 12.7 | 19.1 | 58.5 / 37.4 % |
| Whisper small (sherpa) | 5.7 | 8.4 | 23.0 | 27.5 | 33.3 / 25.2 % |

Accent-insensitive WER is 0.2–1.2 points lower everywhere. On general Spanish, Whistle has
**about twice the WER of Parakeet v3** (9.7 vs 4.3 on FLEURS, 13.9 vs 6.0 on MLS) and is also
behind Whisper small. On our commands, the keyword bias is decisive: 25.5 → 17.9 WER and
**project-name recall 31.7 → 80.5 %**, above Parakeet's 58.5 % (sherpa offers no bias for this
model in offline mode). Many remaining command errors come from the synthetic voices
pronouncing English terms the Spanish way ("auth", "client", "sherpa onnx"), and Parakeet
suffers from the same thing. Real voices will differ.

## 4. Latency (wall time per clip, ms)

| Engine | 3 s P50 / P95 | 10 s P50 / P95 | Commands (2–4 s) P50 / P95 | RTF on FLEURS |
|---|---|---|---|---|
| **Whistle** | **37 / 47** | 501 / 524 | **43 / 65** | 0.052 |
| Parakeet v3 (4 thr) | 85 / 88 | **215 / 224** | 82 / 106 | **0.023** |
| Whisper small (4 thr) | 607 / 1031 | 2576 / 3029 | 873 / 1324 | 0.26 |

**Whistle has a latency cliff between 5 s and 6 s of audio** (median of 5 runs, same clip,
cut to length):

| Audio | 2 s | 3 s | 4 s | 5 s | **6 s** | 8 s | 10 s | 15 s | 20 s | 25 s |
|---|---|---|---|---|---|---|---|---|---|---|
| Whistle total | 27 | 45 | 52 | 74 | **309** | 395 | 505 | 773 | 1059 | 1358 |
| Whistle TTFT | 11 | 16 | 22 | 27 | **242** | 322 | 402 | 604 | 805 | 1009 |
| Parakeet v3 | 66 | 87 | 105 | 124 | 143 | 179 | 217 | 320 | 442 | 563 |

Up to 5 s, Whistle is 2–3× faster than Parakeet. Above 5 s the encoder (TTFT) leaves its fast
path and Whistle becomes 2–2.5× *slower* than Parakeet.

**Streaming** (`needle_stream_transcribe_process`, 1 s chunks): each call re-decodes the whole
buffer, so per-call cost follows the same curve: 24, 33, 53, 60, 88 ms for the first five
seconds, then 326 ms at 6 s, 530 ms at 10 s and 1121 ms at 20 s. A call costs more than 1 s of
audio after about 18 s, so live captions fall behind on long holds. `stream_stop` is then
~0 ms, because the tail is already decoded.

## 5. Decision: **go (conditional)** for Whistle as the default, bundled STT

- Use it for **push-to-talk commands** (the M2–M4 use case). It is 17 MB, loads in 12 ms, and
  answers in **37–74 ms for clips ≤ 5 s**. With the project-name bias it has the best
  project-name recall of the three engines.
- **Conditions** that the `voice-worker` implementation must meet:
  1. Always force `language` from settings. Auto-detect costs 0.9–1.6 WER points on commands.
  2. Always pass the keyword bias: "Pando" + project names + agent ids, rebuilt when projects change.
  3. **Never send > 5 s in one call.** Cut PTT audio at VAD pauses into ≤ 5 s pieces and
     transcribe them as they close, so the final latency after key-up stays one ≤ 5 s pass.
     Limit live streaming captions to the first ~15 s, then switch to segment-wise captions.
  4. Keep libneedle in its own process, never next to sherpa-onnx (S3, R4).
- **Parakeet v3 (sherpa-onnx) is the accuracy engine**: half the WER on general Spanish and
  faster than Whistle on long audio. It is an **optional download** (~650 MB, CC-BY-4.0) for
  dictation and long utterances, and the automatic engine on Intel Macs (R2). Selection policy
  and trait shape: [ADR 0008](../adr/0008-stt-engines-whistle-default-parakeet-optional.md).
- **Whisper small: dropped.** It is worse than Parakeet on every set and 10× slower (RTF 0.26).

## 6. Pending / follow-ups

- **macos-arm64** link and latency run (static `libc++` is the system one there, plus
  `-framework Accelerate` if needed). Needs a Mac (hardware-dependent).
- **Real Spanish voices for the command set** (≥ 5 speakers, laptop mic) and **Common Voice
  es** once the user accepts the Mozilla Data Collective terms. The synthetic command WER is
  indicative only.
- Ask Cactus whether the > 5 s cliff is by design (encoder window or attention kernel) and
  whether a thread-count knob can enter the C API (the CLI has `--threads`).
- Check whether sherpa-onnx hotwords work with Parakeet TDT (`modified_beam_search`) to close
  its project-name recall gap.
- A production `needle-sys` crate: keep the hand-written bindings, add a test that checks
  them against the pinned header's SHA-256.
