---
id: PDA-SP-0011
type: spec
title: Voice invocation (hands-free)
status: backlog
author: mcp
labels: [voice-in, wake-word]
created: 2026-10-11T21:41:32Z
updated: 2026-10-11T22:04:42Z
requirements:
  R1:
    status: backlog
  R2:
    status: backlog
  R3:
    status: backlog
  R4:
    status: backlog
---

## Purpose
Wake the orb by saying "Oye Pando" without touching the keyboard, with minimal CPU cost and few false triggers.

## Scope
Optional continuous listening, Silero VAD, short-pass Whistle verification with bias, fuzzy match; optional custom openWakeWord model; `sonora` AEC so it does not wake on its own voice.

Refs: `docs/research/voice-stack.md` §2, §4.5.

## Requirements

### PDA-SP-0011.R1 — Optional, low-cost continuous listening

Hands-free mode SHALL be off by default, clearly indicated when on (tray + header), capture continuously into a ring, run Silero VAD v5 on 32 ms frames, and only invoke STT on VAD segments. Idle CPU SHALL stay < 3 % of one core on the reference laptop. Audio SHALL never leave the machine or be stored unless the user enables debug recording.

#### Scenario: silent room
- GIVEN hands-free on for 10 min in silence
- THEN STT is never invoked and CPU stays under budget

### PDA-SP-0011.R2 — "Oye Pando" detection

On a VAD segment the detector SHALL transcribe ≤ 2.5 s (300 ms pre-roll) with keyword bias `Pando\nOye Pando\nHey Pando` and accept when the first 1–2 words fuzzy-match the wake phrase above a per-user threshold. The remainder of the utterance SHALL be processed as the command ("Pando, ¿qué hace bitácora?"). A trained openWakeWord model MAY replace the first stage behind the same trait.

#### Scenario: single utterance
- WHEN the user says "Oye Pando, cancela migración auth"
- THEN the wake is accepted and the decision layer receives "cancela migración auth"

### PDA-SP-0011.R3 — Wake-word quality

The wake detector SHALL achieve FAR < 1 per hour on 3 h of office/podcast audio and FRR < 10 % at 2 m, measured by a reproducible evaluation script over recorded fixtures, with a sensitivity setting exposed to the user.

#### Scenario: TV in background
- GIVEN a podcast playing for 1 h
- THEN at most one false wake occurs

### PDA-SP-0011.R4 — Echo cancellation

With feature `echo-cancel` the capture path SHALL run `sonora` AEC3 (+NS) using the exact played PCM as render reference in 10 ms frames, so that VAD and wake detection operate on the cleaned signal; during TTS a raised VAD threshold, ≥ 200 ms speech, 12 dB ducking and stop after 400 ms SHALL implement hands-free barge-in. Without AEC and without a headset hands-free barge-in SHALL be disabled.

#### Scenario: assistant speaking through speakers
- GIVEN 5 min of TTS through laptop speakers
- THEN no self-triggered wake or barge-in occurs
