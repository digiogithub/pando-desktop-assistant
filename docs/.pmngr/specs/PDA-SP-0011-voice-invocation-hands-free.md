---
id: PDA-SP-0011
type: spec
title: Voice invocation (hands-free)
status: backlog
author: mcp
labels: [voice-in, wake-word]
created: 2026-10-11T21:41:32Z
updated: 2026-10-12T08:58:29Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/pando-stt/src/vad.rs#EnergySegmenter
        - crates/pando-stt/src/handsfree.rs#HandsFree
        - crates/app/src/voice/vad.rs#EnergyVad
        - crates/app/src/voice/service.rs#VoiceService::set_hands_free
      tests:
        - crates/pando-stt/src/handsfree.rs#silence_never_invokes_the_stt
        - crates/pando-stt/src/vad.rs#ten_minutes_of_silence_is_cheap
        - crates/pando-stt/src/vad.rs#speech_burst_becomes_one_segment_with_preroll_and_hangover
  R2:
    status: backlog
    trace:
      code:
        - crates/pando-stt/src/wake.rs#match_wake
        - crates/pando-stt/src/wake.rs#SttWakeVerifier
        - crates/pando-stt/src/handsfree.rs#HandsFree
      tests:
        - crates/pando-stt/src/wake.rs#spec_scenario_single_utterance
        - crates/pando-stt/src/handsfree.rs#short_wake_with_command_in_one_utterance
        - crates/pando-stt/src/handsfree.rs#bare_wake_then_command_in_the_next_segment
        - crates/app/src/voice/session.rs#a_hands_free_utterance_starts_a_turn_without_push_to_talk
  R3:
    status: backlog
    trace:
      code:
        - crates/pando-stt/src/wake.rs#WakeConfig
        - crates/pando-stt/src/bin/wake_eval.rs
      tests:
        - crates/pando-stt/src/wake.rs#confusers_are_rejected_at_default_sensitivity
        - crates/pando-stt/src/wake.rs#sensitivity_moves_the_threshold
        - crates/pando-stt/src/handsfree.rs#unrelated_speech_is_ignored
  R4:
    status: backlog
    trace:
      code:
        - crates/audio/src/aec.rs#SonoraAec
        - crates/audio/src/aec.rs#EchoCancel
        - crates/audio/src/bargein.rs#BargeIn
        - crates/audio/src/playback.rs#ReferenceTap
      tests:
        - crates/audio/src/aec.rs#plumbing_cancels_the_played_reference_end_to_end
        - crates/audio/src/aec.rs#erle_meets_the_spike_s4_target
        - crates/audio/src/bargein.rs#disabled_without_aec_or_headset
        - crates/app/src/voice/session.rs#a_hands_free_barge_in_stops_the_answer_and_goes_idle
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
