# 0008 · STT engines: Whistle default for commands, Parakeet v3 optional, Whisper dropped

- **Status:** proposed
- **Date:** 2026-10-12
- **Deciders:** claude-opus (spike S1), pending project owner
- **Refs:** PDA-T-0018, PDA-T-0019, PDA-T-0022, [S1](../spikes/S1-whistle.md), [S2](../spikes/S2-windows-stt-sidecar.md), [S5](../spikes/S5-wake-word.md), `docs/research/voice-stack.md` §1, R1/R2/R5

## Context

S1 measured the three STT candidates on Spanish (FLEURS es, MLS es, 150 synthetic commands with
project names) on an x86_64 desktop CPU:

- **Whistle** (libneedle, 17 MB): WER 9.7 FLEURS / 13.9 MLS. On commands, 25.5 without bias
  and **17.9 with keyword bias** (project-name recall **80 %**). **37–74 ms for clips ≤ 5 s**,
  but a cliff above 5 s: 309 ms at 6 s, 505 ms at 10 s, 1.36 s at 25 s.
- **Parakeet-TDT-0.6b-v3 int8** (sherpa-onnx, ~650 MB): WER **4.3 / 6.0**, commands 12.7
  (project-name recall 58 %), latency linear in length: 85 ms at 3 s, 215 ms at 10 s.
- **Whisper small** (sherpa-onnx): WER 5.7 / 8.4, commands 23.0, RTF 0.26 (2.6 s for 10 s).
- The Whistle engine is closed-source, process-global, about two weeks old, and has no
  macOS x86_64 build (R1, R2). It must not share a process with sherpa-onnx (S3, R4).

## Decision

We will ship **Whistle as the default, bundled STT for push-to-talk commands** and the wake
verifier (S5), with `language` forced from settings, a project-name keyword bias, and **no
single call longer than 5 s** (PTT audio is cut at VAD pauses into ≤ 5 s segments).

We will offer **Parakeet v3 through sherpa-onnx as an optional download**, used for dictation
or utterances longer than ~5 s when installed, when the user picks "high accuracy", and
automatically on Intel Macs.

Both engines sit behind one `Stt` trait in `voice-worker`, each in the process ADR 0004/0007
assigns it (Whistle in `pando-stt`; Parakeet with sherpa-onnx). **Whisper is dropped.**

## Consequences

- Small default install (17 MB model) with the lowest latency for the dominant use case.
  Project names are recognised best thanks to the bias.
- General Spanish dictation through Whistle is noticeably worse (about 2× Parakeet's WER).
  The UI must make the "high accuracy" download easy to discover.
- `voice-worker` needs segmentation logic (VAD, ≤ 5 s) and engine routing by expected length.
- Revisit if Cactus removes the > 5 s cliff or publishes better Spanish weights, if real-voice
  command WER (pending recordings) reverses the ranking, or if Parakeet gains hotword bias
  with equal latency.

## Alternatives considered

| Option | Why not |
|---|---|
| Parakeet v3 as the only/default engine | 650 MB download before first use, worse project-name recall without bias, ~2× slower on short commands |
| Whistle only | 2× WER on general Spanish and slow beyond 5 s; no Intel-Mac build |
| Whisper small / turbo | Worse than Parakeet on all sets and ~10× slower on CPU |
