# 02 · Voice pipeline

> Reconciled with [`research/voice-stack.md`](research/voice-stack.md) on 2026-10-12
> (PDA-T-0016). That note has the full evidence, versions and risk table.

```
            ── app process (pando-assistant) ──────────────────────────   ── pando-voice worker ──
mic ──cpal──▶ rtrb ring ──rubato 5 (FFT)──▶ 16 kHz mono f32 ──voice-proto──▶ Silero VAD (sherpa-onnx)
                  │                                                         Whistle (needle C API)
                  └── RMS tap ──▶ organism.listening amplitude                       │
                                                                       transcript + language (JSON)
                                                                                     │
                                                                        decision layer (03, in app)
                                                                                     │
speaker ◀──cpal◀── rtrb ring ◀── PCM ◀──voice-proto── Piper (sherpa-onnx) ◀── text to speak
                  │
                  └── RMS tap ──▶ organism.speaking amplitude
```

Capture and playback stay in the app (RMS for the organism without IPC); all inference runs
in the separate `pando-voice` process ([ADR 0004](adr/0004-voice-worker-separate-process.md)).

## Speech-to-text: Whistle (Cactus Compute)

Facts from the release post (2026-10-02, https://cactuscompute.com/blog/whistle) and the
shipped `needle.h` (verified in `research/voice-stack.md` §1):

- One 16.9 MB `.cact` file; runs on CPU in the same C++ engine as Needle.
- Input: **16 kHz mono f32, any length**: up to 30 s runs as one pass; longer audio is
  windowed by the engine at sentence ends. Languages: en, de, fr, **es**, it, nl, pl;
  auto-detected or forced (force `es` when the setting says Spanish).
- Output: **JSON text written into a caller buffer** (`{"text","language","ttft_ms",
  "decode_tps"}` plus `"words":[{word,start,end,probability}]` when requested). Silence
  gives an empty text.
- **Keyword biasing**: newline-separated phrases → pass project names, agent ids and
  "Pando" so they transcribe correctly.
- **Streaming exists**: `needle_stream_transcribe_process` (~1 s chunks, commits words two
  passes agree on) and `needle_stream_transcribe_stop` — good for live captions during PTT.
- Decoder depth (`--audio-depth`) is a **CLI flag only; the C API has no depth parameter**.
- **Process-global, not thread-safe**, one speech model per process; spawns its own threads.
- Prebuilt engines exist only for the published targets: **no macOS x86_64**; Linux builds
  use **clang + libc++**; the Windows build is **llvm-mingw (GNU ABI)**. Engine source is not public.
- Weights: `huggingface.co/Cactus-Compute/whistle`; engine: `huggingface.co/Cactus-Compute/needle3`.
  Pin commit SHAs.

### Rust binding plan (`crates/voice-worker`, module/crate `needle-sys`)

- `build.rs`: locate `vendor/needle/<target>/` (env `NEEDLE_DIR` overrides; fetched by
  `scripts/fetch-needle.sh`), link `static=needle` + **`c++` and `c++abi`** (prefer static
  libc++ ≥ 21) + `pthread` + `m` on Linux, system `c++` on macOS. **Not** `stdc++`.
  The header is tiny: committed hand-written bindings are fine (no libclang at build time).
- Real C API (subset):
  ```c
  int  needle_load(const unsigned char* cact, unsigned long long n);   // bytes, not a path
  const char* needle_last_error(void);
  int  needle_transcribe(const float* pcm, int samples, const char* language, const char* keywords,
                         int word_timestamps, char* out, int out_capacity);
  int  needle_stream_transcribe_process(const float* pcm, int samples, const char* language,
                         const char* keywords, char* out, int out_capacity);
  int  needle_stream_transcribe_stop(char* out, int out_capacity);
  int  needle_embed(const char* input, const float* pcm, int samples, float* out, int out_capacity);
  ```
- Safe wrapper owned by **one thread** in the worker (not `Sync`; a global guard enforces a
  single instance); output buffer 256 KiB:
  ```rust
  pub struct Whistle { _guard: InstanceGuard }
  impl Whistle {
      pub fn load(cact: &[u8]) -> Result<Self>;
      pub fn transcribe(&mut self, pcm16k: &[f32], opts: &Opts) -> Result<Transcript>; // language, keywords, word_timestamps
      pub fn stream_process(&mut self, chunk: &[f32], opts: &Opts) -> Result<Partial>;
      pub fn stream_stop(&mut self) -> Result<Transcript>;
  }
  ```
- The app receives `Transcript { text, lang, words, ttft_ms }` over `voice-proto`.

### Capture and chunking

- `cpal` default input at the device's **native** rate (do not ask the device for 16 kHz) →
  `rtrb` → worker thread → `rubato` 5.x **FFT fixed-ratio** resampler (48→16 kHz; sinc only
  for odd rates) → 16 kHz mono f32. Rebuild the stream on `StreamInvalidated`/`DeviceChanged`.
- Push-to-talk is the primary mode: record while held, transcribe on release. No manual
  30 s split (the engine windows long audio). Default key is configurable (⌥/Alt+Space
  collides with OS menus); `global-hotkey` covers Windows/macOS/X11, Wayland uses the
  GlobalShortcuts portal via `ashpd`, with a `pando-assistant ptt` CLI/IPC fallback.
- Hands-free (later milestone): Silero VAD in the worker → short Whistle pass with keyword
  bias "Pando" (wake verification). Energy VAD (RMS over 20 ms frames, hangover 600 ms) is
  the zero-model fallback.
- RMS of each 20 ms frame → `organism.amp` target (normalise against a slow-tracking peak).

## Text-to-speech

- Runtime: **sherpa-onnx `OfflineTts`** in `pando-voice` (the only ONNX Runtime,
  [ADR 0005](adr/0005-sherpa-onnx-single-onnx-runtime.md)). It wraps Piper and Kokoro,
  including the espeak-ng phonemizer (GPL-3, confined to the worker).
- Default voice **Piper `es_ES-davefx-medium` (CC0)**; `es_ES-sharvard-medium` (CC-BY,
  needs attribution) and an `es_MX` voice as options; Kokoro (`ef_dora`, `em_alex`)
  optional behind `tts-kokoro`. Spanish quality comparison: spike PDA-T-0020.
- Keep a `trait Tts { fn synth(&self, text, lang) -> Stream<PcmChunk> }`.
- **Stream by sentence**: synthesise sentence *n+1* while *n* plays; keep the first chunk
  short. Normalise text in Rust first (numbers, ids like `migracion-auth` → "migración
  auth", code blocks → "bloque de código omitido").
- What is spoken: the **final assistant text** of the run, accumulated from
  `content_delta` until `done` (Pando emits no `response` event), or — for delegated work —
  the parent's reply in the resurrected run / the `summary` of the `[delegated-result …]`
  block in session history (04). Structured conclusions over HTTP need gap G3 (PDA-T-0157).
- Playback through `cpal` from an `rtrb` ring; an RMS tap on the output drives `organism.speaking`.

## Barge-in

If the user presses PTT while TTS plays: stop playback within one audio callback (< 20 ms,
atomic flag + drain the ring), keep the transcript. Hands-free barge-in needs headphones or
acoustic echo cancellation: **`sonora`** AEC3 with the played PCM as reference (feature
`echo-cancel`), then VAD on the cleaned signal → duck 12 dB → stop if speech continues
400 ms. Without AEC, disable hands-free while speaking and rely on push-to-talk.

## Latency budget (target)

| Step | Target |
|---|---|
| key release → transcript | < 150 ms for ≤ 10 s audio (measured in spike PDA-T-0018) |
| transcript → decision | < 100 ms (local tev1) |
| decision → API call done | < 50 ms |
| reply text → first audio | < 300 ms |
