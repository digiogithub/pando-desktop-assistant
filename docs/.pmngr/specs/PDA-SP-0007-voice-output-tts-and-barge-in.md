---
id: PDA-SP-0007
type: spec
title: Voice output (TTS and barge-in)
status: backlog
author: mcp
labels: [voice-out]
created: 2026-10-11T21:41:32Z
updated: 2026-10-11T22:03:58Z
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
---

## Purpose
Speak to the user only what matters (conclusions and answers), with low latency, and stop as soon as the user starts speaking.

## Scope
`Tts` trait; Piper `es_ES-davefx-medium` via `sherpa-onnx` in a worker (isolates espeak-ng GPL); Spanish text normalization; sentence-level synthesis; speech queue; output RMS; barge-in via PTT (v1) and via VAD+AEC (M9).

Refs: `docs/02-voice-pipeline.md`, `docs/06-ui-mapping.md`, `docs/research/voice-stack.md` §3, §4.5.

## Requirements

### PDA-SP-0007.R1 — Swappable TTS engine in a worker

Synthesis SHALL run behind `trait Tts { synth(text, lang) -> stream of PCM chunks }` inside the `pando-voice` worker (keeping espeak-ng, GPL-3, out of the app binary), defaulting to Piper `es_ES-davefx-medium` (CC0) through `sherpa-onnx`, with Kokoro and other Piper voices selectable. Only one ONNX Runtime provider (sherpa-onnx) SHALL be linked.

#### Scenario: voice switch
- WHEN the user picks `es_MX-ald-medium` in settings
- THEN the next utterance uses it without restarting the app

### PDA-SP-0007.R2 — Sentence-level synthesis and latency

Text SHALL be split into sentences (respecting abbreviations and numbers; first chunk cut at a comma if > 12 words), sentence n+1 synthesised while n plays, PCM pushed to an `rtrb` ring consumed by the `cpal` output callback. Text → first audio SHALL be < 300 ms (P50).

#### Scenario: long conclusion
- GIVEN a 5-sentence summary
- THEN audio starts < 300 ms after the text is available and has no gaps between sentences

### PDA-SP-0007.R3 — Spanish text normalization

Before synthesis the text SHALL be normalised in Rust: markdown stripped, fenced code replaced by "bloque de código omitido", inline code and ids humanised (`migracion-auth` → "migración auth"), numbers/units/percentages expanded, URLs shortened to their host, and emoji removed.

#### Scenario: markdown answer
- GIVEN "He cambiado `handlers_chat.go` (3 ficheros) ```go …```"
- THEN the spoken text is "He cambiado handlers chat punto go, tres ficheros. Bloque de código omitido."

### PDA-SP-0007.R4 — Speech queue

Utterances SHALL go through a priority queue (direct reply > requested conclusion > notification), never interrupt each other, wait while the body is Listening, and be skippable ("calla", Esc, mute toggle). Muted mode SHALL still show the text.

#### Scenario: mute
- GIVEN TTS muted
- WHEN a conclusion arrives
- THEN it is shown as text and the cell pulses but nothing plays

### PDA-SP-0007.R5 — Output amplitude and Speaking state

Playback SHALL tap the output buffer to publish `tts_rms` (0..1) synchronised with what is audible, set the body to Speaking on first audio and back to Idle when the queue drains.

#### Scenario: end of speech
- WHEN the last sample plays
- THEN the body returns to Idle within 100 ms

### PDA-SP-0007.R6 — Barge-in

Pressing push-to-talk (or saying the wake word in hands-free mode with AEC) while speaking SHALL stop playback within 50 ms by an atomic flag that drains the ring, discard queued sentences of that utterance and start Listening. Without AEC and without headphones, hands-free barge-in SHALL be disabled.

#### Scenario: PTT barge-in
- GIVEN the assistant is speaking
- WHEN the user presses PTT
- THEN audio stops within 50 ms and the body is Listening
