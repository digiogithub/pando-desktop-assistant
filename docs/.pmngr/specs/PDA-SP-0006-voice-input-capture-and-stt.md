---
id: PDA-SP-0006
type: spec
title: Voice input (capture and STT)
status: backlog
author: mcp
labels: [voice-in]
created: 2026-10-11T21:41:32Z
updated: 2026-10-11T22:03:57Z
requirements:
  R1:
    status: backlog
  R2:
    status: backlog
  R3:
    status: backlog
  R4:
    status: backlog
  R5:
    status: backlog
  R6:
    status: backlog
  R7:
    status: backlog
  R8:
    status: backlog
---

## Purpose
Convert the user's voice into text, locally, fast and getting project and agent names right.

## Scope
`cpal` 0.18 → `rtrb` → `rubato` 5 → 16 kHz mono f32; RMS for the organism; push-to-talk; Whistle (`libneedle`) in the `pando-voice` worker (separate process; required on Windows due to the GNU ABI); `sherpa-onnx` Parakeet v3 fallback (macOS Intel and degradation). `Stt` trait.

Refs: `docs/research/voice-stack.md` §1, §4.

## Requirements

### PDA-SP-0006.R1 — Audio capture and resampling

Capture SHALL open the selected (default) input device at its native rate with `cpal` 0.18, never allocate or lock in the callback (push into an `rtrb` ring), resample on a worker thread with `rubato` 5 (FFT fixed-ratio for 48→16 kHz) to 16 kHz mono f32, and rebuild the stream on `DeviceChanged`/`StreamInvalidated`. Host selection (PipeWire/Pulse/ALSA) SHALL be available in settings.

#### Scenario: Bluetooth headset plugged mid-session
- WHEN the default input changes
- THEN capture resumes on the new device within 1 s without restarting the app

### PDA-SP-0006.R2 — Microphone amplitude for the organism

The capture path SHALL compute RMS per 20 ms frame, normalise it against a slow-tracking peak with a noise gate into 0..1, and publish it at ≥ 50 Hz as `mic_rms` without blocking the audio thread.

#### Scenario: whisper vs shout
- THEN `mic_rms` stays within 0..1 and visibly differs between both

### PDA-SP-0006.R3 — Push-to-talk

Push-to-talk SHALL record while the key or mic button is held (with 300 ms pre-roll from the ring), transcribe on release, and drive the body Idle → Listening → (Thinking | Idle). A press while TTS plays SHALL stop playback first (barge-in). Holding longer than 120 s SHALL stop recording with a notice.

#### Scenario: short tap
- GIVEN a press shorter than 250 ms
- THEN nothing is transcribed and the body returns to Idle

### PDA-SP-0006.R4 — STT Whistle in an isolated worker

Speech-to-text SHALL run behind `trait Stt` in a separate `pando-voice` worker process that owns the single non-thread-safe Whistle instance (`needle_load` from bytes, `needle_transcribe` → JSON `{text, language, ttft_ms, decode_tps, words?}`), talking to the app over a length-prefixed stdio protocol (PCM in, JSON out). The app SHALL restart a crashed worker with backoff and surface "voz no disponible" without crashing. On Windows the worker SHALL be built for `x86_64-pc-windows-gnullvm` to link the GNU-ABI `libneedle.a`.

#### Scenario: worker crash
- WHEN the worker dies during a transcription
- THEN the app reports the failure, restarts it, and the next PTT works

### PDA-SP-0006.R5 — Keyword biasing with projects and agents

Each transcription SHALL pass a newline-separated keyword list built from project names, live agent ids/labels, "Pando", "mesnada" and user-defined terms, rebuilt whenever projects or cells change. Language SHALL be forced from settings (default `es`) unless set to auto.

#### Scenario: project name
- GIVEN project `migracion-auth`
- WHEN the user says "cancela migración auth"
- THEN the transcript contains the id `migracion-auth` or a form the decision layer matches

### PDA-SP-0006.R6 — Latency and quality

Key release → final transcript SHALL take < 150 ms (P50) for ≤ 10 s of audio on the reference x86 laptop. Spanish WER SHALL be measured on Common Voice/FLEURS es and an own 50-command set, published in `docs/spikes/`, and re-run in CI on a small fixture set to catch regressions.

#### Scenario: regression gate
- WHEN a model or engine version changes
- THEN the fixture WER must not worsen by more than 2 points

### PDA-SP-0006.R7 — STT fallback

When Whistle is unavailable (macOS x86_64, load failure, user choice) the app SHALL use a fallback `Stt` implementation through `sherpa-onnx` (Parakeet-TDT-0.6b-v3 int8, downloaded on consent) and show the active engine in the header (`stt · whistle` / `stt · parakeet`).

#### Scenario: Intel Mac
- GIVEN macOS x86_64
- THEN the first-run wizard offers the Parakeet download and Whistle is never loaded

### PDA-SP-0006.R8 — Microphone permissions

The app SHALL detect microphone permission denial (`PermissionDenied` on macOS/Windows/portal) and show guidance with a deep link to the OS privacy settings; the macOS bundle SHALL declare `NSMicrophoneUsageDescription` and the `com.apple.security.device.audio-input` entitlement.

#### Scenario: denied on macOS
- THEN a sheet explains how to enable the microphone and the rest of the app keeps working
