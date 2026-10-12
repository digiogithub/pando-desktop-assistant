# 0007 · espeak-ng (GPL-3) is confined to a dedicated `pando-tts` process; STT stays in a permissive worker

- **Status:** accepted (2026-10-12, project owner). When accepted it supersedes the
  "one `pando-voice` worker" part of [0004](0004-voice-worker-separate-process.md); everything else in 0004 stands.
- **Date:** 2026-10-12
- **Deciders:** project owner (sign-off pending), claude-opus (spike S7)
- **Refs:** PDA-T-0023, PDA-T-0020, PDA-T-0019, PDA-T-0151, [`docs/spikes/S7-espeak-ng-licence.md`](../spikes/S7-espeak-ng-licence.md),
  [`docs/spikes/S3-sherpa-onnx-single-runtime.md`](../spikes/S3-sherpa-onnx-single-runtime.md), [`docs/research/voice-stack.md`](../research/voice-stack.md) §3.3

## Context

- espeak-ng, the phonemizer behind Piper and Kokoro in sherpa-onnx, is **GPL-3.0-or-later**.
  Linking it statically or dynamically makes the binary a GPL-3 combined work (FSF GPL FAQ
  `#GPLStaticVsDynamic`). Upstream Piper itself moved to `OHF-Voice/piper1-gpl` (GPL-3.0)
  because it embeds espeak-ng.
- Spike S3 verified that **any** binary linking sherpa-onnx's prebuilt static libs contains
  espeak-ng, even if it only uses the VAD (180 `espeak_*` symbols in a VAD-only binary): the
  C API object references the TTS code. Only a source build with
  `-DSHERPA_ONNX_ENABLE_TTS=OFF` drops it.
- ADR 0004 puts STT (Whistle, `libneedle.a`) and TTS (espeak-ng) in one `pando-voice` binary.
  libneedle is a **closed-source binary** (Apache-2.0 licence, no source published). A GPL-3
  combined work may only be conveyed with its **Corresponding Source** (GPL-3 §6), which would
  include libneedle's source. We cannot provide it, so **a binary containing both libneedle and
  espeak-ng cannot be distributed** in compliance with GPL-3. ADR 0004's consequence "our code
  inside it stays MIT and releases ship its corresponding source" missed this.
- Separately, S3 found that on Windows sherpa-onnx's prebuilt static libs are MSVC while
  libneedle is GNU ABI (llvm-mingw): they cannot share a binary there without building
  sherpa-onnx from source; and on Linux sharing one binary needs a symbol-privatised libneedle
  to keep C++ exceptions working.
- GPL-3 §5 treats separate, independent programs distributed together as an **aggregate**, and
  the FSF FAQ (`#MereAggregation`, `#GPLPlugins`) says pipes, sockets and command-line arguments
  are normally communication between separate programs, unless the semantics are intimate
  (exchanging complex internal data structures). Our worker protocol (`voice-proto`) sends text
  and PCM frames with small JSON control messages over stdio.

## Decision

We will split voice inference into **two worker processes** on every OS:

1. **`pando-tts`** (GPL-3.0-or-later as a distributed binary): sherpa-onnx with TTS (Piper,
   Kokoro) and espeak-ng. Input: text + voice id + control messages; output: PCM chunks. It
   contains **no** libneedle and no code that is not available as source.
2. **`pando-stt`** (permissive: our MIT code + Apache-2.0/MIT deps + the libneedle blob):
   Whistle STT, wake-word verification and VAD. It must **not** link the prebuilt sherpa-onnx
   libs. VAD and the fallback STT (Parakeet) come from sherpa-onnx **built from source with
   `SHERPA_ONNX_ENABLE_TTS=OFF`** (consumed through `sherpa-onnx-sys`'s `SHERPA_ONNX_LIB_DIR`), so
   there is still a single ONNX Runtime per process (ADR 0005). On Windows it is built for
   `x86_64-pc-windows-gnullvm` (ADR 0004).

The `app` binary (MIT) talks to both over `voice-proto` at arm's length and contains no GPL code.

Rules for the code:

- Only the `pando-tts` crate may depend on a sherpa-onnx build with TTS; `cargo xtask check-deps`
  forbids `sherpa-onnx` (any build) in `app`, and the prebuilt one in `pando-stt`.
- A release check runs `nm <bin> | grep -c ' espeak_'` and fails if it is non-zero for `app` or
  `pando-stt`.
- `voice-proto` messages stay simple (text, PCM, small JSON); never share memory or internal
  data structures with `pando-tts`.
- Every release that ships `pando-tts` also publishes its **Corresponding Source** at the same
  place and tag (GPL-3 §6(d)): our worker crates and `voice-proto` at that commit, `Cargo.lock`,
  the exact sherpa-onnx tag and its espeak-ng / piper-phonemize sources, the ONNX Runtime
  version's source reference, and the build scripts; plus the GPL-3 text and a written notice in
  the NOTICE file and the About screen.
- The app must keep working if a user replaces `pando-tts` with their own build (configurable
  path; integrity checks may warn but must not refuse it).

## Consequences

- Distribution is legally clean: the GPL-3 binary has complete source; the closed libneedle
  lives in a permissive binary; the app stays MIT.
- One more process (≈ 40–80 MB RSS with Piper loaded) and one more binary to sign and package.
  STT and TTS crash independently, which is also a robustness gain.
- Two ONNX Runtime copies on disk (one per worker), never in the same process.
- Extra CI work: a from-source sherpa-onnx build without TTS for `pando-stt` (all targets,
  including llvm-mingw on Windows). Until it exists, `pando-stt` can use an energy VAD and no
  Parakeet fallback, or the VAD can run in `pando-tts` (it is already there) at the cost of
  sending capture audio to both workers.
- README § License and ADR 0004's consequences must be updated when this ADR is accepted
  (`pando-voice` → `pando-tts`/`pando-stt`); tasks PDA-T-0087/0090/0091/0107 follow the split.
- Revisit if: an in-house Spanish G2P (option C below) removes espeak-ng from the product, or
  Cactus publishes libneedle's source under a GPL-compatible licence, or legal counsel advises
  that stdio IPC with the app is not sufficient separation.

## Alternatives considered

| Option | Why not |
|---|---|
| A. Keep one `pando-voice` with libneedle + espeak-ng (ADR 0004 as written) | Not distributable: GPL-3 Corresponding Source would include libneedle, which is closed. Also needs the libc++ symbol-privatisation hack on Linux and a from-source sherpa-onnx on Windows. |
| B. Relicense the whole product GPL-3 | Same libneedle blocker; drops the MIT goal; buys nothing over the split. |
| C. Own rule-based Spanish G2P in Rust (no espeak-ng) | Viable and attractive (Spanish orthography is regular), but needs a G2P that matches Piper's espeak IPA, plus a lexicon for English voices and loanwords; Kokoro also uses espeak. Keep as a later hardening item; it would let `pando-tts` become permissive. |
| D. Load espeak-ng as a separate shared library (dlopen) inside one worker | Still a combined work per the FSF (`#GPLStaticVsDynamic`); does not solve the libneedle problem. |
| E. Supertonic 3 (no phonemizer) | OpenRAIL-M use restrictions, archived upstream, Spanish quality unknown. |
