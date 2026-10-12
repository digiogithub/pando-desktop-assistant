---
created_at: 2026-10-12T08:13:17.441192193Z
updated_at: 2026-10-12T08:13:17.441192193Z
tags:
    - changes
    - voice
---
# Hands-free pipeline, echo cancellation, wake evaluation and model downloader

Builds on [[pando-assistant/changes/voice-workers-implementation]] and [[S5-wake-word|docs/spikes/S5-wake-word.md]]; plan [[pando-assistant/plans/backlog-execution_plan.md]].

## What changed
- `voice-proto` v2: `ToWorker::HandsFree`, `Audio`; `FromWorker::Utterance`.
- `pando-stt`: `vad` (EnergySegmenter default; SileroSegmenter behind feature `silero-vad`), `wake` (WakeDetector, SttWakeVerifier, fuzzy `match_wake`), `handsfree::HandsFree`, `wake-eval` binary. PDA-T-0136/0137/0141.
- `audio`: `ReferenceTap`, feature `echo-cancel` (`EchoCancel`, `SonoraAec`, `EchoProcessor`), `bargein::BargeIn`. PDA-T-0139.
- `models` crate + `pando-models` CLI + embedded `models.toml`. PDA-T-0148.
- docs: `docs/wake-eval/README.md`, `docs/research/wake-openwakeword-training.md` (PDA-T-0142 recipe).

## Verification
Unit tests per crate, clippy -D warnings, check-deps OK (13 crates). Synthetic wake evaluation (3 h negatives): FAR 0.33/h, FRR@2 m 14 % (target 10 %), idle CPU 0.008 %; Silero worse than energy VAD on that set. AEC ERLE >= 46 dB on synthetic echo, 5 min TTS residual -66 dBFS worst block. Model downloader verified against real HF/GitHub downloads.

## Open
Real recordings for R3, real acoustic AEC measurement, Silero needs the TTS-off sherpa source build (GCC 11 toolchain), app wiring (supervisor, ReferenceTap, models CLI).
