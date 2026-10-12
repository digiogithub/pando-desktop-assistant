---
created_at: 2026-10-11T23:23:01.151190146Z
updated_at: 2026-10-11T23:23:01.151190146Z
tags:
    - pando-assistant
    - spike
    - voice-in
    - wake-word
    - change
---
# Voice spikes S1 (Whistle WER), S2 (Windows gnullvm sidecar), S5 (wake word)

Part of [[pando-assistant/plans/backlog-execution_plan.md]]. Tasks PDA-T-0018, PDA-T-0019 and PDA-T-0022 (story PDA-US-0006) are all **in_review**, with hardware-dependent parts pending. Commit `2ae77b8`.

## What changed
- New standalone spike crate `spikes/whistle-ffi/`, excluded from the workspace and built with `--target-dir target/spikes`:
  - `src/sys.rs`: hand-written `needle.h` bindings.
  - `src/lib.rs`: safe `Whistle` wrapper (`!Send`, one per process).
  - `build.rs`: links static libc++ on Linux and llvm-mingw on Windows.
  - Binaries: `bench`, `stream`, `pando-stt` (stdio sidecar), `stt-host`.
  - `scripts/`: `prep_s1.py`, `sherpa_bench.py`, `wer.py`, `run_s1.sh`, command list and keyword list.
- New `spikes/wake-word/` (`prep_s5.py`, `s5_eval.py`).
- Reports: `docs/spikes/S1-whistle.md`, `S2-windows-stt-sidecar.md`, `S5-wake-word.md`.
- New ADR `docs/adr/0008-stt-engines-whistle-default-parakeet-optional.md` (proposed), added to the ADR index.
- Models and data live in `~/.cache/pando-assistant-spikes/`. None are committed.

## Key numbers
- **S1 WER (%)**, on FLEURS es / MLS es / synthetic commands:

  | Engine | FLEURS es | MLS es | Commands | Project-name recall |
  |---|---|---|---|---|
  | Whistle | 9.7 | 13.9 | 25.5; 17.9 with keyword bias | 80 % |
  | Parakeet v3 | 4.3 | 6.0 | 12.7 | 58 % |
  | Whisper small | 5.7 | 8.4 | 23.0 | — |

- **S1 latency**: Whistle takes 37 ms on a 3 s clip and 501 ms on a 10 s clip. It has a cliff above 5 s (74 ms at 5 s, 309 ms at 6 s). Parakeet takes 85 ms (3 s) and 215 ms (10 s).
- **S1 decision**: use Whistle for PTT commands. Cut audio into segments of 5 s or less, force the language and always pass the keyword bias. Parakeet is an optional accuracy engine. Drop Whisper.
- **S2**: the gnullvm `.exe` links with no extra DLLs. IPC overhead for a 5 s clip under Wine is P95 ≤ 2.3 ms and max ≤ 3 ms, well under the 20 ms target.
- **S5**:
  - **Option A** (VAD → Whistle with keyword bias):
    - false accepts: 0 per hour over 3 h;
    - "Oye Pando" false-reject rate: 3 % at 0.5 m, 11 % at 1 m, 25 % at 2 m (synthetic audio);
    - CPU: 0.2 % idle, about 1.8 % with continuous speech.
  - **Option B** (sherpa keyword spotting with English tokens): misses 98–100 % of wake phrases, so it is rejected.
  - **Option C** (custom openWakeWord model): not trained.

## Why
- Validate the voice stack before building it. These spikes address risks R1, R3, R4, R5 and R14 from [[research/voice-stack.md]].

## Verification
- Real runs on an Intel Core Ultra 9 285:
  - 200 FLEURS clips, 200 MLS clips and 150 + 150 command clips per engine;
  - 3 h of negatives for the wake-word test;
  - the Windows binary run under Wine 11.
- `cargo build` and `cargo clippy` on the spike pass, with one pedantic warning left.
- Pending:
  - S1: run on macOS arm64.
  - S2: test with a real MSVC host and native Windows timings.
  - S5: real recordings.
  - All: Common Voice es (blocked on Mozilla Data Collective terms) and real voices for the command set.

Related: [[pando-assistant/spikes/S3-sherpa-onnx-single-runtime]], [[adr/0004-voice-worker-separate-process]], [[adr/0007-espeak-ng-gpl-isolated-tts-worker]].
