---
created_at: 2026-10-11T22:43:44.306791175Z
updated_at: 2026-10-11T22:43:44.306791175Z
tags:
    - change
    - spike
    - voice
    - licence
---
# Voice spikes S3, S4, S7 (PDA-T-0020, PDA-T-0021, PDA-T-0023)

Date: 2026-10-12 · Agent: claude-opus · Commits: a4cb7cb (S3), 0ef10f4 (S4), 27c621d (S7 + ADR 0007) · Part of [[pando-assistant/plans/backlog-execution_plan.md]]

## What changed
- New standalone spike crates, excluded from the workspace (`exclude = ["spikes"]` in the root Cargo.toml):
  - `spikes/s3-sherpa-tts` with bins `s3-sherpa-tts`, `s3-combo`, `s3-ehtest` and `s3-vadonly`. The libneedle link experiments are in `build.rs`.
  - `spikes/s4-audio-aec`, with subcommands `devices`, `duplex`, `resample`, `aec` and `hotplug`.
- New reports:
  - `docs/spikes/S3-sherpa-onnx-single-runtime.md`
  - `docs/spikes/S4-audio-cpal-rubato-sonora.md`
  - `docs/spikes/S7-espeak-ng-licence.md`
- New ADR `docs/adr/0007-espeak-ng-gpl-isolated-tts-worker.md` (status **proposed**), added to the ADR index.

## Key findings
- **S3:**
  - Piper `es_ES-davefx-medium` fp32 with 2 threads reaches first audio in **132 ms** for a 10-word sentence (RTF 0.04). Kokoro fp32 takes about 500 ms; int8 is slower on x86.
  - Silero VAD costs 53 µs per 32 ms window.
  - Linking libneedle (libc++) next to sherpa-onnx (libstdc++) breaks C++ exceptions, because libc++abi interposes `__cxa_throw`. The fix is a symbol-private libneedle (`ld -r` + `objcopy --keep-global-symbols`, with the personality routine renamed).
  - Windows: sherpa's prebuilt libs are MSVC, so they cannot share a binary with GNU-ABI libneedle.
  - A corrupt .onnx aborts the process, so model SHA-256 must be verified before loading.
  - A VAD-only sherpa binary still contains espeak-ng.
- **S4:**
  - cpal's PipeWire/Pulse hosts are opt-in features and need dev libraries plus libclang.
  - rubato `Fft` 48→16 kHz has a flat passband and costs 1.4 µs per 10 ms.
  - sonora AEC3 costs 40 µs per 10 ms frame and gives 23–31 dB ERLE with 0 false barge-ins on a synthetic echo path.
  - Barge-in is detected about 320 ms after onset when the echo is ≥ 20 dB down.
  - When a device is removed, no error is raised (the PipeWire host stalls), so the audio supervisor needs a watchdog plus rebuild (8 ms).
- **S7:**
  - A worker that contains both libneedle (closed source) and espeak-ng (GPL-3) cannot be distributed, because the Corresponding Source would have to include libneedle.
  - ADR 0007 proposes two workers: `pando-tts` (GPL-3) and `pando-stt` (permissive). Its VAD comes from sherpa-onnx built with `SHERPA_ONNX_ENABLE_TTS=OFF`.
  - The app stays MIT. It would supersede the single-worker part of [[ADR 0004]].

## Why
These are the voice spikes of story PDA-US-0006; they de-risk [[pando-assistant/research/voice-stack]] before the voice milestones.

## How it was verified
- Real runs on an Intel Core Ultra 9 285 with Pop!_OS 24.04 and PipeWire 1.6.8. All numbers are in the reports.
- Spike binaries were built with `cargo build --release --target-dir target/spikes`; `cargo fmt` was applied.
- Licence sources were cross-checked: GPL-3 §5/§6, the FSF GPL FAQ, the espeak-ng README and piper1-gpl.
- Pending (all three tasks are in_review):
  - Other OS targets.
  - Informal MOS.
  - Real acoustic ERLE and the 5-minute run, which need a laptop.
  - Physical unplug.
  - Owner sign-off of ADR 0007.
  - The README § License update, after acceptance.
