# Research: voice stack for pando-assistant (Rust + GPUI, local only, no Python)

Date: 2026-10-11. Scope: STT, wake word / VAD, TTS, audio plumbing, hotkeys/tray/autostart, model
distribution. Inputs: `docs/01-strategy.md`, `docs/02-voice-pipeline.md`, `docs/06-ui-mapping.md`.

Legend: **[verified]** = checked first-hand (file downloaded / registry queried / official page read);
**[verify]** = from memory or secondary sources, so confirm in a spike.

---

## 0. TL;DR

| Component | Recommendation | Fallback |
|---|---|---|
| STT | **Whistle** via `libneedle.a` + bindgen, owned by **one dedicated worker thread** (or a sidecar process on Windows) | `sherpa-onnx` crate 1.13.8 with Parakeet-TDT-0.6b-v3 (25 EU langs) or `whisper-rs` 0.16 (small/turbo). Required on **Intel macOS**, where no Whistle build exists |
| VAD | **Silero VAD v5/v6** (MIT, ~2 MB ONNX), through `sherpa-onnx` or `ort` | Energy VAD (RMS + hangover) as a zero-model fallback |
| Wake word | Phase 1: none (push-to-talk). Phase 2: **VAD → short Whistle pass with keyword bias `Pando`** (no training, no new runtime). Phase 3 (optional): custom openWakeWord/livekit-wakeword ONNX model (`livekit-wakeword` 0.1.3 or `oww-rs` 0.3.3) | sherpa-onnx KWS (English token set only). **Not Porcupine** |
| TTS | **Piper `es_ES-davefx-medium` (CC0)** through `sherpa-onnx` (or `piper-rs` 0.2.0), streamed sentence by sentence | Kokoro-82M (`ef_dora`, `em_alex`) via sherpa-onnx; Supertonic 3 (raw text, no espeak, but OpenRAIL-M and archived) |
| Phonemizer | espeak-ng is **GPL-3.0**. Either accept GPL for that binary, isolate TTS in a separate `pando-tts` process, or write a small **rule-based Spanish G2P** (Spanish spelling is regular) | — |
| Audio I/O | `cpal` **0.18.2** (native PipeWire + PulseAudio hosts since 0.18.0) + `rubato` **5.0.1** + `rtrb` 0.4 / `ringbuf` 0.5 | — |
| AEC / barge-in | **`sonora` 0.2.0** (pure-Rust WebRTC APM port: AEC3, NS, AGC2, BSD-3) | `webrtc-audio-processing` 2.1.0 (C++ via meson; harder on Windows); `aec3` 0.4.0. Default without AEC: barge-in only via PTT key or with headphones |
| Hotkey | `global-hotkey` 0.8.0 (Windows, macOS, X11) + **XDG GlobalShortcuts portal via `ashpd` 0.13.13** on Wayland | CLI/IPC `pando-assistant ptt toggle` bound by the user in the DE's shortcut settings |
| Tray | Linux: **`ksni` 0.3.6** (pure-Rust StatusNotifierItem over D-Bus, no GTK). macOS/Windows: **`tray-icon` 0.26.1** | — |
| Autostart | `auto-launch` 0.6.0 (XDG autostart `.desktop`, LaunchAgent, HKCU Run) | Flatpak: Background portal (`ashpd`) |
| ONNX runtime | **Pick a single ONNX Runtime provider.** Recommended: the official `sherpa-onnx` crate, which bundles its own ORT, for VAD + TTS + fallback STT. Use `ort` 2.0.0-rc.13 only if sherpa is dropped | — |

---

## 1. STT: Whistle / Needle (Cactus Compute)

### 1.1 What actually ships [verified]

- HF engine repo `Cactus-Compute/needle3` (licence tag **Apache-2.0**, LICENSE file present; commit
  `93e88ede867e4311c39151285e74df513e4b835b`) has these platform folders:
  `linux-x86_64`, `linux-arm64`, `linux-armv7`, `linux-riscv64`, `linux-mipsel`, `macos-arm64`,
  `windows-x86_64`, `windows-arm64`, `android-*`, `ios-*`, `tvos-arm64`, `watchos-arm64`, `wasm`,
  `wasm-component`, plus `python/` (wheels).
  - **No `macos-x86_64`**, so Intel Macs need the fallback STT.
  - Each folder holds `needle` (CLI), `libneedle.a` (~1.5–2.3 MB) and `needle.h` (4528 B, identical across folders).
- Weights repo `Cactus-Compute/whistle` (**Apache-2.0**, commit `ca5287601bef25af26dcf2e1b2bdc0843a7c19e5`):
  `whistle.cact` 16,919,407 B, SHA-256 `b6e02f048568ac5d01a2042556c658061e699acbc0aa2a1439f52f3d461dffeb`.
  `config.json` lists weights_version 2.0.0, sample_rate 16000, max_audio_seconds 30, cactus-quants group 128.
- **The C++ engine source is NOT public.** `github.com/cactus-compute/needle` (Apache-2.0) contains only
  the Python package, training/export code and tests, with no `.cpp`. So the engine is a binary-only
  static library under an Apache-2.0 label. We cannot rebuild it for missing targets, for
  `-msvc`, or with sanitizers.
- Telemetry exists only in the Python package (`needle/_telemetry.py`, opt-out via `NEEDLE_TELEMETRY=0`).
  The static lib has **no socket/HTTP/getenv symbols** (checked with `nm -u` on linux-x86_64), so the
  engine cannot phone home.

### 1.2 The real C API (`needle.h`) [verified]

```c
#define NEEDLE_TEXT 1
#define NEEDLE_SPEECH 2
/* One process-global, non-thread-safe model per kind, text and speech.
   Negative returns indicate failure; call needle_last_error(). */
int  needle_load(const unsigned char* cact, unsigned long long n);   // from a memory buffer, not a path
int  needle_models(void);                                              // bitmask of loaded kinds
const char* needle_last_error(void);
int  needle_init(const char* system_prompt, const char* tools_json, const char* tool_index_path); // Needle text model
int  needle_complete(const char* input, const float* pcm, int samples, int max_new_tokens, char* out, int out_capacity);
void needle_set_audio(const char* language, const char* keywords, int word_timestamps);
void needle_reset(void);
int  needle_transcribe(const float* pcm, int samples, const char* language, const char* keywords,
                       int word_timestamps, char* out, int out_capacity);
int  needle_stream_transcribe_process(const float* pcm, int samples, const char* language,
                       const char* keywords, char* out, int out_capacity);
int  needle_stream_transcribe_stop(char* out, int out_capacity);
int  needle_embed(const char* input, const float* pcm, int samples, float* out, int out_capacity);
```

Semantics, quoted or paraphrased from the header:

- **Input**: 16 kHz mono `f32` in [-1, 1], *any length*. Up to 30 s runs as one pass. Longer audio
  runs as consecutive 30 s windows cut at sentence ends, with word times from clip start. That makes
  our "split at lowest-energy window" logic **unnecessary**.
- **Output is JSON text** written into a caller buffer, and the return value is the token count:
  `{"text":"...","language":"es","ttft_ms":0.0,"decode_tps":0.0}` plus
  `"words":[{"word","start","end","probability"}]` when `word_timestamps != 0`.
  Silence or steady noise gives an empty text and language.
- `language`: `"en"|"de"|"fr"|"es"|"it"|"nl"|"pl"` or NULL to auto-detect.
  `keywords`: **newline-separated** phrases (bias list).
- **Streaming exists**: `needle_stream_transcribe_process` takes ~1 s chunks, re-transcribes the buffered
  audio and commits words that two consecutive passes agree on. It returns
  `{"text","words","pending","language","received","pass_ms"}`. `..._stop` flushes the tail.
  Cost: every call re-decodes the buffer, so it is fine for live captions during PTT but too costly
  for always-on listening.
- **Threading**: process-global and not thread-safe. Internally it spawns its own worker threads
  (`pthread_create`, `sched_setaffinity` imported). Only one speech model per process, and no
  way to pick a thread count.
- **Decoder depth**: `--audio-depth` is a CLI flag only. **There is no depth parameter in the C API.**
  Depth variants appear to be produced as separate `.cact` files (`needle build --layers`,
  `download --depth`). [verify with Cactus whether depth-pruned whistle files are published]
- Python wrapper default output buffer is 256 KiB (`1 << 18`). Use the same size.

### 1.3 Linking gotchas [verified by `nm`]

- **Linux libneedle.a is built with clang + libc++** (`std::__1` symbols, ABI tag `ne210100`, i.e.
  libc++ 21), *not* libstdc++. So `cargo:rustc-link-lib=stdc++` as in 02-voice-pipeline.md is wrong.
  Link `c++` + `c++abi` (preferably static `libc++.a`/`libc++abi.a` from LLVM ≥ 21 to avoid a
  runtime dependency on `libc++1` packages) + `pthread` + `m`. Mixing it with libstdc++-based
  deps (onnxruntime inside sherpa-onnx) in one binary normally works because the namespaces differ,
  but exceptions/unwinder and `operator new` are shared. **Spike it.**
- **Windows libneedle.a is an llvm-mingw (GNU ABI, UCRT, libc++) archive** (`__gxx_personality_seh0`,
  `__imp___acrt_iob_func`, `needle.cpp.obj`). It **cannot be linked into an `x86_64-pc-windows-msvc`
  binary**, which is the target GPUI/Zed builds on. Options:
  1. Build a small Rust sidecar `pando-stt.exe` for `x86_64-pc-windows-gnullvm` that links
     libneedle + llvm-mingw libc++, and talk to it over stdio/named pipe with length-prefixed
     PCM in and JSON out. **Recommended.**
  2. Build the entire app for `*-windows-gnullvm` (GPUI support unknown) [verify].
- macOS: `macos-arm64` only. Link `c++` (system libc++) and `-framework Accelerate` if needed
  [verify unresolved symbols].

### 1.4 Recommended integration shape

- `crates/voice-in/needle-sys` (bindgen on the vendored header; the header is tiny, so you could
  commit hand-written bindings and skip bindgen/libclang at build time).
- `Whistle` is **not** `Sync`. Wrap it in a single owner thread (`std::thread` + crossbeam channel)
  that serialises `transcribe`, `stream_*` and `embed`. A global `OnceLock<Mutex<()>>` guard enforces one instance.
- Consider making the worker a **separate process on every OS** (`pando-voice-worker`). That gives
  crash isolation for a closed-source C++ blob, identical code paths with the Windows sidecar, and
  licence isolation for espeak-ng if TTS lives there too (§3.3). The cost is ~1 ms IPC per call.
- Keyword bias: pass project names, agent ids and "Pando" as a `\n`-joined list. Rebuild it whenever the Pando project list changes.
- Force `language="es"` when the user's setting says Spanish. Auto-detect on short clips is the usual failure mode.

### 1.5 Quality / open questions

- No per-language WER is published as text. The benchmark SVG shows FLEURS and MLS **averages** over
  the 7 languages, and Whisper base wins on the MLS average. **Measure Spanish WER ourselves**
  (Common Voice es / MLS es / FLEURS es, plus 50 of our own command utterances with project names).
- Released 2026-10-02, so the engine is about one week old. Expect API churn. Pin the HF commit SHAs above.

### 1.6 Fallback STT options

| Option | Crate | Spanish | Size | Licence | Notes |
|---|---|---|---|---|---|
| **Parakeet-TDT-0.6b-v3** | `sherpa-onnx` 1.13.8 (official, Apache-2.0, static by default, auto-downloads native libs) | 25 EU languages incl. es, auto language ID; strong on es per Open ASR leaderboard [verify numbers] | int8 encoder ~620 MB (+decoder/joiner) | CC-BY-4.0 | Best quality; heavy download; offline (use VAD-segmented "simulated streaming") |
| **Whisper** (small / large-v3-turbo q5) | `whisper-rs` 0.16.0 (whisper.cpp; Metal/Vulkan/CUDA features) | Very good es on turbo/small; base is weak | small q5 ~190 MB, turbo q5 ~550 MB | MIT | Mature; 30 s windows; hallucinates on silence (gate with VAD) |
| Whisper via sherpa | `sherpa-onnx` | same | same-ish | MIT | Keeps one runtime |
| Streaming zipformer | `sherpa-onnx` | No good official Spanish streaming model [verify] | 20–70 MB | Apache-2.0 | Good only for en/zh |
| Moonshine v2 | `sherpa-onnx` | Mainly English; non-English variants under a community/non-commercial licence [verify] | 27–60 MB | MIT (en) | Not suitable for es |

Recommendation: **sherpa-onnx + Parakeet v3 int8** as the quality fallback (also the Intel-Mac path).
`whisper-rs` small is the lighter alternative if a second native runtime is acceptable.

---

## 2. Voice invocation: wake word and VAD

### 2.1 Options

| Option | Rust path | Custom "Pando" | Licence | Status |
|---|---|---|---|---|
| **rustpotter** 3.0.2 | pure Rust (DTW on MFCC "wakeword refs" + small NN "wakeword models") | Yes, record 3–10 samples (refs) or train a model (Rust CLI) | Apache-2.0 | **Unmaintained since 2023-10.** Mediocre false-accept rate with refs |
| **openWakeWord-format models** | `livekit-wakeword` 0.1.3 (Apache-2.0, ort-based, 2026-09) or `oww-rs` 0.3.3 (MIT, tract-onnx, no native ORT) | Yes, but **training needs Python** (synthetic TTS data). Training runs once at dev time and only the ONNX ships | Code Apache/MIT. Upstream openWakeWord *pretrained* models are CC BY-NC-SA, so train our own | Active |
| **sherpa-onnx KWS** | `sherpa-onnx` crate | Open-vocabulary via a token file, no training | Apache-2.0 | Published KWS models are **English (GigaSpeech BPE) and Chinese only**. "Pando" can be spelled with English tokens (`▁PA N DO`) and may work. Spike |
| **Picovoice Porcupine** | `pv_porcupine` (crate placeholder 0.0.0) | Yes (console), Spanish supported | Commercial. **Free tier reportedly discontinued 2026-06-30**; AccessKey validated online at init | Reject (online key check breaks "everything local") |
| **VAD → Whistle keyword pass** | Our own code | Yes, zero training. Bias list `Pando\nOye Pando\nHey Pando` and fuzzy-match the transcript start | Apache-2.0 | Recommended phase-2 approach |
| Whistle embeddings + DTW | `needle_embed` (one row per 80 ms) | Few-shot enrolment like rustpotter but on a stronger encoder | — | Research spike |

### 2.2 Recommended design

1. **v1: push-to-talk only.** It is deterministic and costs no CPU while idle.
2. **v2 hands-free.** Always-on cpal capture at 16 kHz goes to **Silero VAD** (32 ms frames, ~0.1 ms of
   CPU each, <1 % of one core). When speech is detected, take a ≤2.5 s window with 300 ms pre-roll →
   `needle_transcribe(lang="es", keywords="Pando\n…")` → normalise → Levenshtein match on the first 1–2 words.
   On a match, the rest of the utterance becomes the command (single-utterance "Pando, ¿qué hace bitácora?").
   CPU: Whistle runs only on VAD hits (~6 ms TTFT for 5 s on M4 Pro; expect 30–80 ms on a mid x86).
   Silence-gate behaviour means false VAD triggers are cheap.
3. **v3 (optional).** If false accepts or rejects are a problem, train a custom openWakeWord model
   ("oye pando" / "hey pando") with the livekit-wakeword toolkit (Python at **build time only**), and
   ship the ~200 KB ONNX + shared melspec/embedding models (~2.5 MB). Inference: `livekit-wakeword`
   (if we are on `ort`) or `oww-rs` (tract, no ORT). Cost is ~2–5 % of one core on x86 [verify].

### 2.3 VAD choice

- **Silero VAD v5/v6** (MIT, ~2.2 MB ONNX). Through `sherpa-onnx` (built in) or `ort`
  (`voice_activity_detector` 0.2.1 wraps it; licence field "non-standard", so check before use).
  Avoid the `silero-vad` 0.1.0 crate (GPL-2.0).
- **TEN VAD** (`ten-vad` 0.1.0): very low latency, but upstream licence is Apache-2.0 *with additional
  conditions* (non-compete) [verify]. Avoid.
- **Energy VAD**: keep it as a fallback and for the organism amplitude. It is not robust enough for hands-free in noisy rooms.
- `sonora` includes WebRTC's RNN VAD (inside AGC2) and NS, which are useful as a pre-filter.

---

## 3. TTS

### 3.1 Engines

| Engine | Rust path | Spanish voices | Size | Licence (weights) | Notes |
|---|---|---|---|---|---|
| **Piper (VITS)** | `piper-rs` 0.2.0 (MIT wrapper; **pins `ort =2.0.0-rc.12`**; builds espeak-ng from a git submodule via `espeak-rs-sys`) or `sherpa-onnx` | `es_ES-davefx-medium` **CC0**, 63.2 MB, sha256 `6658b03b…e222a917`; `es_ES-sharvard-medium` CC-BY-3.0, 76.7 MB; `es_MX-claude-high` Apache-2.0; `es_MX-ald-medium` Unlicense; `es_AR-daniela-high` CC-BY-SA-4.0 | 60–110 MB/voice | per voice (above) | RTF ~0.05–0.1 on desktop CPU; first audio for a short sentence 50–150 ms |
| **Kokoro-82M** | `sherpa-onnx` (kokoro-multi-lang), `kokoro-tts` 0.3.3 | `ef_dora`, `em_alex`, `em_santa`. **Ungraded / little es training data**, so likely worse than in English | fp32 ~330 MB, int8 ~90 MB + voices | Apache-2.0 | Better prosody in English; uses espeak-ng for es |
| Supertonic 3 | ONNX via `ort` (official Rust example in repo) | `es` among 31 languages; **raw text input, no phonemizer** | ~99M params | **OpenRAIL-M** (use restrictions) | **Repo archived 2026-09-09**, no support |
| Kitten / Pocket TTS / Matcha | sherpa-onnx | English (and a few others) only | — | — | Not for es |

### 3.2 Recommendation

- Default **Piper `es_ES-davefx-medium` (CC0)**. Offer `es_ES-sharvard-medium` (needs attribution) and an
  `es_MX` voice as options. English: `en_US-lessac-medium` / `en_GB-alba-medium`.
- Runtime: **sherpa-onnx `OfflineTts`**. It handles Piper and Kokoro behind one API, including
  sentence splitting and callbacks, and shares ORT with VAD and fallback STT. Use `piper-rs` only if we commit to
  `ort` directly. Note its exact `ort` pin, which can conflict with our own `ort` version.
- **Streaming by sentence**: split the conclusion on `[.!?;:]` plus newlines (respect abbreviations and numbers),
  synthesise sentence *n+1* while *n* plays, and push PCM into an `rtrb` ring consumed by the cpal output
  callback. Time to first audio is bounded by the first sentence. Keep the first chunk short (cut the first
  sentence at a comma if it is longer than ~12 words). Target < 300 ms is achievable with Piper medium on CPU.
- Text normalisation for es (numbers, ids like `migracion-auth` → "migración auth", code block → "bloque de
  código omitido") happens in Rust before TTS.

### 3.3 espeak-ng / GPL

- espeak-ng is **GPL-3.0-or-later**. Both `piper-rs` (via `espeak-rs-sys`, statically compiled) and `sherpa-onnx`
  (Piper/Kokoro frontends) link it, so the resulting binary is a GPL-3 combined work. The crate
  licences ("MIT", "Apache-2.0") do not change that.
- Options:
  1. **Accept GPL-3** for the assistant binary (fine if pando-assistant is open source under a compatible licence).
  2. **Process isolation**: put TTS in its own executable (`pando-tts`, GPL-3) driven over stdio. That is
     usually treated as aggregation [verify with counsel]. It fits the sidecar pattern from §1.4.
  3. **Own Spanish G2P in Rust**: Spanish orthography→IPA is close to deterministic (stress rules, `c/z/ll/y/x`,
     diphthongs). A ~500-line rule set can emit the espeak-style IPA that Piper's `phoneme_id_map` expects
     for `es` voices. English would still need espeak or a lexicon (CMUdict is BSD-ish), or the
     English voices could live only in the GPL sidecar.
  4. Supertonic 3 (no phonemizer), at the cost of the OpenRAIL-M licence and an archived project.
- Recommendation: **(2) now, (3) as a later hardening item** if a closed licence is ever needed.

---

## 4. Audio plumbing

### 4.1 cpal 0.18.2 (2026-08-16) [verified via CHANGELOG]

- 0.18.0 (2026-06-06) added native **PipeWire and PulseAudio hosts**. Default host priority is PipeWire → Pulse → ALSA.
  It also unified `Error`/`ErrorKind` with `PermissionDenied`, `DeviceChanged`, `DeviceNotAvailable` and `StreamInvalidated`,
  WASAPI auto-reroutes to a new default device, and CoreAudio reports device changes. 0.18.2 added `Xrun` reporting.
- Practical: handle `StreamInvalidated`/`DeviceChanged` by rebuilding the stream (Bluetooth headset hot-plug).
  Ask for the device's native rate and let rubato convert. Do **not** request 16 kHz from the device (many reject it,
  and BT HFP switches profile).
- The PipeWire host is new (4 months old). Keep the ALSA/Pulse fallback selectable in settings.
- Never allocate or lock in callbacks. Capture goes into `rtrb` → worker thread → `rubato` → VAD/STT.

### 4.2 Resampling: `rubato` 5.0.1 (2026-10-01)

- v5 reworked the API (use `Async`/`Fft` resamplers with the `audioadapter` buffer traits). The plan text
  mentions `SincFixedIn` (v0.x API). **Re-check names against 5.x docs.** For 48k→16k an **FFT
  fixed-ratio resampler** is cheapest; use sinc only for odd device rates.

### 4.3 macOS microphone permission

- Bundle `Info.plist` with `NSMicrophoneUsageDescription` (otherwise the process is killed on first capture).
- Hardened runtime (needed for notarisation): entitlement `com.apple.security.device.audio-input` = true.
  If sandboxed (Mac App Store only), the same key applies under App Sandbox. Developer ID distribution does not need the sandbox.
- Unbundled binaries launched from Terminal inherit Terminal's TCC permission, so test the `.app` bundle path.
- cpal 0.18 surfaces denial as `ErrorKind::PermissionDenied`. Show a GPUI sheet with a deep link
  `x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone`.
- Global hotkey via Carbon `RegisterEventHotKey` (what `global-hotkey` uses) needs **no** Accessibility permission.

### 4.4 Windows

- WASAPI shared mode, usually 48 kHz f32. The Settings → Privacy → Microphone toggle ("Let desktop apps access")
  shows up as a capture failure, so detect it and show guidance.
- The Whistle link constraint (GNU ABI) pushes STT into a sidecar (see §1.3).

### 4.5 Echo cancellation and barge-in

| Crate | Version | Type | Notes |
|---|---|---|---|
| **sonora** | 0.2.0 (2026-07-29), BSD-3 | **Pure-Rust port** of WebRTC APM M145 (+fixes to M156): AEC3, NS, AGC2, HPF | Claims the 2400+ C++ reference tests pass; ~1.07× C++ cost at 16 kHz (4.2 µs/10 ms frame on M4 Max); CI on Linux/macOS/Windows; MSRV 1.91; "AI-assisted" port, 91 stars, so verify robustness |
| webrtc-audio-processing (tonarino) | 2.1.0 (2026-05-13) | C++ wrapper (`bundled` feature builds with meson) | Proven, but meson/ninja + C++ toolchain on all 3 OSes; Windows builds are painful |
| aec3 | 0.4.0 (2026-09-16) | Rust AEC3-only port | Smaller scope |

Barge-in strategy:

1. Always: **pressing PTT while speaking stops TTS** within one audio callback (<20 ms) by setting an atomic flag and draining the ring.
2. Hands-free while speaking: feed the **exact played PCM** (render reference, 10 ms frames at 16 or 48 kHz) and mic
   frames to sonora AEC3 → Silero VAD on the *cleaned* signal with a raised threshold and ≥200 ms speech
   → duck TTS by 12 dB → if the speech continues 400 ms, stop and start capture for the turn.
   Re-use the pre-roll buffer so the first syllables are not lost.
3. If there is no AEC (feature off) and the output device is not a headset, disable hands-free barge-in.
   On macOS, the `VoiceProcessingIO` audio unit provides system AEC [future, needs CoreAudio code outside cpal].

---

## 5. Global hotkey, tray, autostart

### 5.1 Hotkeys

- `global-hotkey` **0.8.0**: Windows, macOS, **Linux X11 only**. macOS requires the manager and an event
  loop on the main thread (GPUI owns it; integrate via GPUI's platform run loop). Windows needs a
  Win32 message loop on the creating thread. Emits `Pressed`/`Released` states, which push-to-talk needs.
  Check that Windows `Released` is reliable [verify].
- **Wayland**: use the **XDG GlobalShortcuts portal** through `ashpd` 0.13.13 (`desktop::global_shortcuts`):
  `create_session` → `bind_shortcuts([{id:"ptt", description, preferred_trigger:"<Alt>space"}])` → listen to
  `Activated` / `Deactivated` signals (the Deactivated signal gives key release, so hold-to-talk works).
  Support: **KDE Plasma (since 5.27/6)**, **GNOME 48+** (backend landed in xdg-desktop-portal-gnome 48.rc),
  **Hyprland** (own portal; ignores the preferred trigger, so the user binds it). **Not** in wlroots/Sway, Cosmic or xapp portals [verify current].
  The compositor shows its own approval dialog and the user may pick a different key.
- Fallback for unsupported compositors: a single-instance IPC (Unix socket) and a CLI `pando-assistant ptt
  {start|stop|toggle}` that the user binds in Sway/niri config. Avoid reading `/dev/input` (needs the `input` group, security smell).
- Default key: `⌥Space` collides with GNOME/KDE "window menu" (Alt+Space), Windows system menu / PowerToys Run,
  and the ChatGPT/Raycast-type launchers on macOS. Make it configurable and consider `Ctrl+Alt+Space` / `Super+Space` alternatives per OS.

### 5.2 Tray

- `tray-icon` 0.26.1 (Tauri): macOS/Windows are good. On Linux it uses libappindicator/ayatana + **GTK3**
  and needs a GTK main loop. GPUI on Linux does not run GTK, so you would need an extra thread with `gtk::main()`.
- **`ksni` 0.3.6**: pure-Rust StatusNotifierItem/DBusMenu over zbus. Works on KDE, XFCE, Cinnamon, and
  GNOME with the AppIndicator extension (on by default in Ubuntu/Fedora? Ubuntu yes, Fedora no). **Use ksni on Linux, tray-icon elsewhere.**
- Tray menu: Listen (PTT toggle), Hands-free on/off, Mute TTS, Open window, Quit. The icon state mirrors the organism (idle/listening/speaking).

### 5.3 Autostart

- `auto-launch` 0.6.0: Linux XDG `~/.config/autostart/*.desktop`, macOS LaunchAgent plist (or AppleScript
  login item), Windows `HKCU\…\Run`. For macOS 13+, `SMAppService.mainApp.register()` is the modern API (via `objc2-service-management`) [verify crate].
- Flatpak: request it via the Background portal (`ashpd::desktop::background`).

---

## 6. Model distribution

### 6.1 Policy

- **Bundle**: the needle engine (static lib, linked in), Silero VAD (~2 MB), Whistle (16.9 MB, small enough
  to bundle so first run works offline). A small default Piper voice could ship too, but it costs ~63 MB per voice.
- **Download on first use** (with explicit consent UI, progress and resume): TTS voices beyond the default, Kokoro,
  fallback STT (Parakeet ~650 MB / Whisper), and the optional wake-word model.
- Pin by **HF commit SHA** URLs: `https://huggingface.co/<repo>/resolve/<sha>/<path>`. Verify
  **SHA-256** (HF LFS `oid` *is* the file's SHA-256, available from `api/models/<repo>/tree/<sha>/<dir>`).
  Bake a manifest (`models.toml`: id, url, sha256, size, licence, attribution) into the binary.
- Download with `reqwest` (rustls) + `sha2` 0.11, streaming to `*.part` then atomic rename. `hf-hub` 1.0.0
  is an option, but plain HTTP with our own manifest is simpler and auditable.
- Paths (`directories` 6.0 `ProjectDirs::from("dev","Pando","pando-assistant")`):
  - Linux: `$XDG_DATA_HOME/pando-assistant/models` (default `~/.local/share/...`)
  - macOS: `~/Library/Application Support/dev.Pando.pando-assistant/models`
  - Windows: `%LOCALAPPDATA%\Pando\pando-assistant\data\models`
- Air-gapped: `PANDO_ASSISTANT_MODELS_DIR` override + `pando-assistant models import <dir>`.

### 6.2 Size and licence table

| Artifact | Size | SHA-256 (pinned) | Licence | Attribution |
|---|---|---|---|---|
| needle engine `libneedle.a` linux-x86_64 | 2.20 MB | `5f8ed375f304cf7e7ba9e6e37d105b51549597e014afbd006407a0836804e5cd` | Apache-2.0 (binary-only) | NOTICE |
| needle engine linux-arm64 | 2.02 MB | `3847620903354d3965c52eb319d3b36acf0a7a1cbe09c16d56e9f6b612ab6fbc` | Apache-2.0 | |
| needle engine macos-arm64 | 1.54 MB | `c1b955778dcb79c92efaf5606f94e9d3a4ed85e093096b41e502bc9748a90614` | Apache-2.0 | |
| needle engine windows-x86_64 (GNU ABI) | 2.34 MB | `ef057b317fbc3103758e295052a5c717a695589b379e2202aa4a7e879e425672` | Apache-2.0 | |
| `whistle.cact` | 16.9 MB | `b6e02f048568ac5d01a2042556c658061e699acbc0aa2a1439f52f3d461dffeb` | Apache-2.0 | |
| Silero VAD v5 ONNX | ~2.2 MB | (pin at spike) | MIT | |
| Piper `es_ES-davefx-medium` | 63.2 MB | `6658b03b1a6c316ee4c265a9896abc1393353c2d9e1bca7d66c2c442e222a917` | CC0 | none required |
| Piper `es_ES-sharvard-medium` | 76.7 MB | `40febfb1679c69a4505ff311dc136e121e3419a13a290ef264fdf43ddedd0fb1` | CC-BY-3.0 | required |
| Piper `es_MX-claude-high` / `es_MX-ald-medium` | ~110 / ~63 MB | (pin) | Apache-2.0 / Unlicense | |
| Kokoro-82M int8 + voices | ~90–110 MB | (pin) | Apache-2.0 | |
| Parakeet-TDT-0.6b-v3 int8 (sherpa) | ~650 MB | (pin) | CC-BY-4.0 | required |
| Whisper small q5 / turbo q5 | ~190 / ~550 MB | (pin) | MIT | |
| Custom openWakeWord model (ours) | ~0.2 MB + 2.5 MB shared feature models | ours | ours (train data licences apply) | |
| espeak-ng data (inside TTS sidecar) | ~1–10 MB (es+en only) | — | **GPL-3.0+** | source offer |
| onnxruntime (via sherpa-onnx / ort) | 10–20 MB | — | MIT | NOTICE |

---

## 7. Crate version summary (crates.io, 2026-10-11)

`cpal 0.18.2` · `rubato 5.0.1` · `rtrb 0.4.0` / `ringbuf 0.5.3` · `sherpa-onnx 1.13.8` (official k2-fsa) ·
`sherpa-rs 0.6.8` (community, last update 2025-10, superseded) · `ort 2.0.0-rc.13` · `piper-rs 0.2.0` (ort =rc.12) ·
`espeak-rs 0.2.0` · `kokoro-tts 0.3.3` · `whisper-rs 0.16.0` · `sonora 0.2.0` · `aec3 0.4.0` ·
`webrtc-audio-processing 2.1.0` · `livekit-wakeword 0.1.3` · `oww-rs 0.3.3` · `tract-onnx 0.23.8` · `rustpotter 3.0.2` (stale) ·
`voice_activity_detector 0.2.1` · `global-hotkey 0.8.0` · `ashpd 0.13.13` · `tray-icon 0.26.1` · `ksni 0.3.6` ·
`auto-launch 0.6.0` · `directories 6.0.0` · `sha2 0.11.0` · `hf-hub 1.0.0` · `gpui 0.2.2` (crates.io; Zed git usually newer).

---

## 8. Risk table

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Whistle engine is **closed-source binary**, released 9 days ago; API/ABI churn, no fixes we can make | High | High | Pin SHAs; wrap behind `trait Stt`; keep sherpa-onnx Parakeet fallback compiled in behind a feature |
| R2 | **No macOS x86_64** build of libneedle | Certain | Medium | Intel Macs use fallback STT automatically |
| R3 | **Windows libneedle is GNU/llvm-mingw ABI**; GPUI app is MSVC, so it cannot be linked | Certain | High | `pando-stt` sidecar built for `x86_64-pc-windows-gnullvm`; spike first |
| R4 | Linux lib needs **libc++** (not libstdc++); mixing with libstdc++ (onnxruntime) in one binary | Medium | Medium | Static libc++/libc++abi; or sidecar; CI link test on Ubuntu 22.04/24.04, Fedora, Debian 12 |
| R5 | Whistle Spanish WER unknown (only 7-lang averages; Whisper base wins MLS avg) | Medium | High | Spike S1 measures es WER + command set; keyword bias; force `es` |
| R6 | Engine is process-global / non-thread-safe; one model per process | Certain | Low | Single owner thread or worker process |
| R7 | espeak-ng **GPL-3** via piper-rs/sherpa-onnx | Certain | Medium–High (licensing) | TTS sidecar process, or own Spanish G2P, or accept GPL |
| R8 | Two ONNX Runtimes (`ort` + sherpa's bundled ORT) clash at link/load time | Medium | Medium | Choose one provider (sherpa-onnx) |
| R9 | Wayland global shortcuts not available (wlroots/Sway/Cosmic) | Medium | Medium | Portal where present; CLI/IPC binding fallback; in-window PTT |
| R10 | `⌥Space` conflicts on every OS | High | Low | Configurable; per-OS default |
| R11 | Echo leak → false barge-in, or TTS self-transcription in hands-free | High without AEC | Medium | sonora AEC3 + VAD on cleaned signal + duck-then-stop; PTT-only barge-in when AEC off |
| R12 | sonora is young (AI-assisted port, 91★) | Medium | Medium | Fallback to `webrtc-audio-processing` behind the same trait; record AEC test fixtures |
| R13 | cpal PipeWire host is new (0.18.0, June 2026) | Medium | Medium | Host selection in settings; fall back to Pulse/ALSA on error |
| R14 | Wake word false accepts/rejects for "Pando" (short, 2 syllables, common es word fragment "pan", "panda") | High | Medium | Prefer "Oye Pando"/"Hey Pando"; VAD+Whistle verification; per-user threshold |
| R15 | Porcupine requires online key; free tier ended | Certain | — | Do not use |
| R16 | Kokoro Spanish quality weak/ungraded | Medium | Low | Piper default; Kokoro optional |
| R17 | macOS TCC denial / missing Info.plist kills process | Medium | High | Ship as `.app`, entitlement, graceful `PermissionDenied` UX |
| R18 | Model downloads (Parakeet 650 MB) on metered/offline machines | Medium | Low | Bundle Whistle+VAD+1 voice; explicit consent; import from dir |
| R19 | Supertonic archived; OpenRAIL-M | Certain | Low | Only as experiment |

---

## 9. Suggested spikes (time-boxed)

| Spike | Goal | Exit criteria | Est. |
|---|---|---|---|
| **S1 Whistle FFI + Spanish quality** | `needle-sys` + safe wrapper on linux-x86_64 & macos-arm64; transcribe Common Voice es (200 clips), FLEURS es, and 50 own commands with project-name bias | Links with static libc++; es WER measured vs Parakeet v3 and Whisper small; P50/P95 latency for 3 s/10 s clips on x86 laptop | 2 d |
| **S2 Windows link** | Build `pando-stt` for `x86_64-pc-windows-gnullvm` linking `libneedle.a`; stdio protocol to an MSVC GPUI host | Round-trip 5 s clip < 20 ms overhead | 1–2 d |
| **S3 Single ORT provider** | `sherpa-onnx` 1.13.8 static: Silero VAD + Piper davefx + Kokoro es in one binary alongside libneedle | Builds on Linux x86_64/aarch64, macOS arm64, Windows; first-audio < 300 ms for a 10-word sentence | 1–2 d |
| **S4 Audio + AEC** | cpal 0.18 capture/playback (PipeWire, Pulse, CoreAudio, WASAPI), rubato 5 FFT 48→16 k, sonora AEC3 with TTS reference | Barge-in detection while speaker plays; ERLE measured; no false barge-in on 5 min TTS playback | 2–3 d |
| **S5 Wake word** | VAD→Whistle "Pando" verification vs sherpa KWS English tokens vs custom oWW model | FAR < 1/hour on 3 h of office/podcast audio, FRR < 10 % at 2 m distance; idle CPU < 3 % of one core | 2–3 d |
| **S6 Hotkeys/tray on Wayland** | `ashpd` GlobalShortcuts PTT (Activated/Deactivated) on GNOME 48+ and KDE 6; `ksni` tray; `global-hotkey` on X11/macOS/Windows incl. key release | Hold-to-talk works on all listed; documented fallback for Sway | 1–2 d |
| S7 Licence decision | GPL espeak-ng: sidecar vs own es G2P | Written decision; if G2P: prototype vs espeak IPA on 1k sentences | 0.5–3 d |

---

## 10. Corrections to `docs/02-voice-pipeline.md`

- C API is **`needle_load(buf, len)`** (bytes, not a path), returns JSON via caller buffer; **no depth argument**.
- Streaming **is** available (`needle_stream_transcribe_process/stop`).
- Any length accepted (auto-windowing beyond 30 s), so drop our own 30 s split.
- Link **libc++ / libc++abi**, not `stdc++`. Windows artifact is GNU-ABI. No macOS x86_64.
- Engine source is not public. "Prebuilt for 17 targets" is accurate, but those are the only builds.
- `rubato` 5.x API differs from `SincFixedIn` naming in the doc.
- `global-hotkey` is X11-only on Linux. Add the ashpd portal path.

## Sources

- Whistle blog: https://cactuscompute.com/blog/whistle
- Weights: https://huggingface.co/Cactus-Compute/whistle · Engine: https://huggingface.co/Cactus-Compute/needle3 · Repo: https://github.com/cactus-compute/needle
- Devices guide: https://cactuscompute.com/blog/needle-supported-devices
- sherpa-onnx Rust examples: https://github.com/k2-fsa/sherpa-onnx/tree/master/rust-api-examples
- piper-rs: https://github.com/thewh1teagle/piper-rs · Piper voices: https://huggingface.co/rhasspy/piper-voices
- Supertonic: https://github.com/supertone-inc/supertonic · https://huggingface.co/Supertone/supertonic-2
- Kokoro voices: https://replicate.com/alphanumericuser/kokoro-82m/readme
- OpenPhonemizer: https://github.com/NeuralVox/OpenPhonemizer
- livekit-wakeword: https://docs.livekit.io/agents/multimodality/audio/wakeword/ · oww-rs: https://docs.rs/crate/oww-rs
- Porcupine free-tier end: https://community.home-assistant.io/t/fyi-picovoice-confirmed-free-tier-accesskeys-will-stop-working-after-june-30-2026/1012744
- sherpa KWS model card: https://huggingface.co/openEuler/sherpa-kws
- cpal changelog: https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md
- sonora: https://github.com/dignifiedquire/sonora
- global-hotkey: https://github.com/tauri-apps/global-hotkey
- GlobalShortcuts portal status: https://dec05eba.com/2024/03/29/wayland-global-hotkeys-shortcut-is-mostly-useless/ · https://github.com/YaLTeR/niri/pull/1165 · https://wiki.linuxfromscratch.org/blfs/ticket/21291
- Parakeet v3: https://catalog.ngc.nvidia.com/orgs/nvidia/-/collections/parakeet-tdt-0.6b/ · https://huggingface.co/CoderViking/parakeet-tdt-0.6b-v3-onnx
