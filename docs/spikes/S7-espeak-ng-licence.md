# Spike S7 · espeak-ng licence decision (GPL-3)

- **Task:** PDA-T-0023 (story PDA-US-0006) · **Date:** 2026-10-12 · **Status:** decision proposed, owner sign-off pending
- **Decision record:** [ADR 0007](../adr/0007-espeak-ng-gpl-isolated-tts-worker.md) (proposed)
- **Builds on:** [`docs/research/voice-stack.md`](../research/voice-stack.md) §3.3, [ADR 0004](../adr/0004-voice-worker-separate-process.md), [S3](S3-sherpa-onnx-single-runtime.md)

Not legal advice. The conclusions below follow the FSF's published interpretation and the licence
texts; have counsel confirm before the first public binary release.

## Facts checked

| Fact | Source(s) |
|---|---|
| espeak-ng is **GPL-3.0-or-later** | espeak-ng `README.md` § License ("released under the GPL version 3 or later"); GitHub licence metadata `gpl-3.0` |
| Linking (static **or** dynamic) a GPL module makes a combined work covered by the GPL as a whole | FSF GPL FAQ `#GPLStaticVsDynamic` ("Linking a GPL covered work statically or dynamically with other modules is making a combined work…") |
| Separate programs talking over pipes/sockets/command line are normally separate works; intimate exchange of internal data structures can make them one program | FSF GPL FAQ `#MereAggregation`, `#GPLPlugins` (fork/exec plug-ins are separate programs unless the communication is intimate) |
| An aggregate of separate, independent works on one medium does not extend the GPL to the other parts | GPL-3 §5, last paragraph ("aggregate") |
| Conveying object code requires the **Corresponding Source** (all source needed to build and run it, including linked libraries that are not System Libraries); §6(d) allows offering it from the same download place | GPL-3 §1 (definition of Corresponding Source), §6 |
| Piper itself relicensed to GPL-3 because it embeds espeak-ng | `rhasspy/piper` README: "Development has moved: OHF-Voice/piper1-gpl"; `piper1-gpl` licence metadata `gpl-3.0`, README "embeds espeak-ng for phonemization" |
| `piper-phonemize` (the glue sherpa-onnx uses) is MIT; sherpa-onnx is Apache-2.0 | GitHub licence metadata of `rhasspy/piper-phonemize`, `k2-fsa/sherpa-onnx` |
| Any binary that links the prebuilt sherpa-onnx static libs contains espeak-ng, even if only VAD is used | **Measured in S3**: `s3-vadonly` has 180 `espeak_*` symbols |
| sherpa-onnx can be built without TTS (and without espeak-ng) | sherpa-onnx v1.13.8 `CMakeLists.txt`: `option(SHERPA_ONNX_ENABLE_TTS … ON)`, `include(espeak-ng-for-piper)` only when ON |
| libneedle (Whistle engine) is a closed-source static library, Apache-2.0 binary-only | `voice-stack.md` §1, §6.2 |

## Analysis

1. **The app binary (MIT)** never links sherpa-onnx or espeak-ng (ADR 0004). It talks to the
   workers over stdio with text, PCM and small JSON messages: separate programs by the FSF's own
   criterion. The MIT licence of the app is unaffected.
2. **A worker that links espeak-ng is GPL-3 as distributed.** Our own code in it can stay MIT
   (MIT is GPL-compatible), but whoever ships the binary must offer the Corresponding Source of
   the whole worker.
3. **The worker of ADR 0004 cannot be shipped as designed.** It would contain libneedle *and*
   espeak-ng; the Corresponding Source would have to include libneedle's source, which does not
   exist publicly. Apache-2.0 is GPL-3-compatible as a licence, but compatibility does not help
   when the source is missing. (libneedle is not a "System Library" under GPL-3 §1.)
4. Consequently STT (libneedle) and TTS (espeak-ng) must live in **different binaries**, unless
   espeak-ng is removed from the product (own G2P).

## Options compared

| | A. Isolated GPL TTS worker (recommended) | B. Own Spanish G2P in Rust | C. GPL for everything |
|---|---|---|---|
| App licence | MIT | MIT | GPL-3 |
| Worker(s) | `pando-tts` GPL-3, `pando-stt` permissive + libneedle | one permissive worker possible | — |
| libneedle compatible | Yes (kept out of the GPL binary) | Yes | **No** (closed blob inside a GPL work) |
| Effort | Process split + source-offer in release pipeline + a no-TTS sherpa-onnx build for `pando-stt` | ~500-line rule G2P matching espeak IPA for `es`, accuracy test on ≥ 1k sentences; English voices still need a lexicon; Kokoro also uses espeak | none technically, but blocked |
| Risk | Low (standard practice; GPL text + FSF FAQ) | Quality regressions in pronunciation | Blocked by libneedle |

## Decision (proposed)

**Option A**, recorded in [ADR 0007](../adr/0007-espeak-ng-gpl-isolated-tts-worker.md):
`pando-tts` (sherpa-onnx + Piper/Kokoro + espeak-ng, GPL-3) and `pando-stt` (Whistle + VAD,
permissive) as two processes; the app stays MIT. Option B stays on the roadmap as hardening.

### Distribution implications (source offer)

For every release that ships `pando-tts`:

- Publish, next to the binaries and under the same tag, a source archive (or tagged repo link)
  containing: `crates/voice-worker` (or the future `pando-tts` crate), `crates/voice-proto`,
  workspace `Cargo.toml`/`Cargo.lock`, vendored crate sources (`cargo vendor`) for that binary,
  the sherpa-onnx source at the exact tag (with its espeak-ng fork and piper-phonemize), the
  ONNX Runtime source reference/version used by that sherpa-onnx release, and the build scripts
  (`xtask bundle`). GPL-3 §6(d) allows this "equivalent access from the same place".
- Ship `COPYING` (GPL-3 text) with `pando-tts`, list it in the NOTICE file (PDA-T-0151) and show
  the licence in the About screen.
- Do not prevent users from replacing `pando-tts` with their own build (configurable path, no
  hard signature lock).
- espeak-ng data files (`espeak-ng-data/`) shipped with voices are part of the GPL work: same
  source offer.
- Models and voices keep their own licences (Piper davefx CC0, Kokoro Apache-2.0) and are not
  affected by the GPL of the program that reads them.

## What changes elsewhere (after sign-off)

- README § License: "linked only into the separate `pando-voice` worker" → `pando-tts`; mention
  that the STT worker contains the closed-source libneedle under Apache-2.0. **Not edited yet**:
  the ADR is still proposed.
- ADR 0004 gets `superseded by 0007` for the single-worker part; tasks PDA-T-0087/0090/0091/0107
  are adjusted to two workers; a new task covers the no-TTS sherpa-onnx build for `pando-stt`
  and the release source-offer step.
- `xtask check-deps`: forbid `sherpa-onnx` in `app`; release check `nm | grep espeak_` = 0 for
  `app` and `pando-stt`.

## Pending

- **Project-owner sign-off** of ADR 0007 (acceptance criterion of PDA-T-0023).
- Optional: counsel review before the first public binary release.
