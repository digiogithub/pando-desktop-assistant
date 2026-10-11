# 02 · Voice pipeline

```
mic ──cpal──▶ ring buffer ──rubato──▶ 16 kHz mono f32 ──VAD/chunk──▶ Whistle (needle C API)
                  │                                                     │
                  └── RMS tap ──▶ organism.listening amplitude          └──▶ transcript + language
                                                                                 │
                                                                           decision layer (03)
                                                                                 │
   speaker ◀──cpal◀── f32 PCM ◀── Piper ONNX (ort) ◀── text to speak ◀── Pando conclusion
                  │
                  └── RMS tap ──▶ organism.speaking amplitude
```

## Speech-to-text: Whistle (Cactus Compute)

Facts from the release post (2026-10-02, https://cactuscompute.com/blog/whistle):

- One 16.9 MB `.cact` file; runs on CPU with no dependencies in the same C++ engine as Needle.
- Input: **16 kHz mono, up to 30 s per pass**. Languages: en, de, fr, **es**, it, nl, pl;
  language auto-detected or forced.
- Outputs: transcript, per-word timestamps with probabilities, and an encoder embedding
  (one row per 80 ms frame).
- **Keyword biasing**: an Aho-Corasick automaton over phrases you pass lifts their log
  probability during beam search → pass project names and agent ids
  (`bitacora`, `gintrack`, `migracion-auth`, …) so they transcribe correctly.
- Silence gate: below a loudness threshold it returns an empty transcript without decoding.
- `--audio-depth` selects decoder depth (2..8 layers); the encoder always runs all 8.
- Benchmarks (M4 Pro, 10 s clip): 11.1 ms to first token, 1319 tok/s.
- Engine ships prebuilt for 17 targets; each folder has `needle`, **`libneedle.a` and
  `needle.h`**. "`needle_load`, `needle_transcribe` and `needle_embed` are the whole speech
  C API." The engine reads no env vars. Weights: `huggingface.co/Cactus-Compute/whistle`;
  engine: `huggingface.co/Cactus-Compute/needle3`; source: `github.com/cactus-compute/needle`.

### Rust binding plan (`crates/voice-in`)

- `build.rs`: locate `vendor/needle/<target>/` (env `NEEDLE_DIR` overrides), run `bindgen`
  on `needle.h`, `cargo:rustc-link-lib=static=needle` + the C++ runtime (`stdc++` on Linux,
  `c++` on macOS). Fail with a clear message pointing to `scripts/fetch-needle.sh`.
- Safe wrapper:
  ```rust
  pub struct Whistle { h: *mut needle_t }
  impl Whistle {
      pub fn load(path: &Path, depth: u8) -> Result<Self>;
      pub fn transcribe(&self, pcm16k: &[f32], opts: &Opts) -> Result<Transcript>; // opts: language, keywords, word_timestamps
      pub fn embed(&self, pcm16k: &[f32]) -> Result<Vec<[f32; D]>>;
  }
  unsafe impl Send for Whistle {}
  ```
  Exact signatures/structs come from `needle.h` — read it first; the JSON-returning CLI
  shape (`audio_text`, `audio_language`, `function_calls`) hints the C API may return JSON
  text. **[verify]**
- Run transcription on a dedicated thread; the UI gets `Transcript { text, lang, words,
  ttft_ms }` over a channel.

### Capture and chunking

- `cpal` default input, any rate → `rubato` (SincFixedIn) → 16 kHz mono f32.
- Push-to-talk (⌥ Space, `global-hotkey`) is the primary mode: record while held, transcribe on
  release. Clamp to 30 s; if longer, split at the lowest-energy 200 ms window.
- Optional hands-free mode: simple energy VAD (RMS over 20 ms frames, hangover 600 ms).
  Whistle's own silence gate makes false triggers cheap.
- RMS of each 20 ms frame → `organism.amp` target (normalise against a slow-tracking peak).

## Text-to-speech

- **Piper** voices are ONNX + a JSON config (phoneme ids, sample rate). Runtime: `ort` crate
  (ONNX Runtime). Phonemisation needs `espeak-ng` data; the `piper-rs` crate bundles a
  phonemizer — evaluate it first; otherwise `sherpa-rs` (sherpa-onnx bindings) wraps the
  whole Piper pipeline in C++ and is the pragmatic fallback. Candidate voices:
  `es_ES-davefx-medium`, `es_ES-sharvard-medium`. **[verify quality]**
- Alternative with better prosody: **Kokoro** (ONNX, also via `ort`/sherpa). Keep a
  `trait Tts { fn synth(&self, text, lang) -> Stream<PcmChunk> }` so both fit.
- What is spoken: **only the conclusion**. Pando subagents end with a thin
  `<pando:conclusion>` block (see 04); the assistant speaks that block, or for a direct chat
  turn the last assistant message, trimmed of code blocks (read "bloque de código omitido").
- Playback through `cpal`; an RMS tap on the output buffer drives `organism.speaking`.

## Barge-in

If the user starts speaking while TTS plays: stop playback within 50 ms, keep the transcript.
Detecting speech while the speaker is on requires either headphones or acoustic echo
cancellation — `webrtc-audio-processing` (feature `echo-cancel`). Without it, disable
hands-free mode while speaking and rely on push-to-talk.

## Latency budget (target)

| Step | Target |
|---|---|
| key release → transcript | < 150 ms for ≤ 10 s audio |
| transcript → decision | < 100 ms (local tev1) |
| decision → API call done | < 50 ms |
| conclusion → first audio | < 300 ms |
