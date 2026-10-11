# 0005 · `sherpa-onnx` is the only ONNX Runtime provider

- **Status:** accepted
- **Date:** 2026-10-12
- **Deciders:** project owner, planning agents
- **Refs:** PDA-T-0020, PDA-T-0093, PDA-T-0107, PDA-T-0136, [`docs/research/voice-stack.md`](../research/voice-stack.md) §0, §3.2, §8 (R8)

## Context

Silero VAD, Piper/Kokoro TTS and the fallback STT (Parakeet) are all ONNX models. The
`sherpa-onnx` crate (official k2-fsa, 1.13.8) bundles its own ONNX Runtime and wraps all of
them behind one API (including Piper's phonemizer and sentence callbacks). `piper-rs` pins
`ort =2.0.0-rc.12`; adding `ort` next to sherpa would put two ONNX Runtimes in one binary,
which risks link- and load-time clashes.

## Decision

Inside `pando-voice`, every ONNX model runs through `sherpa-onnx`. We do not depend on
`ort`, `piper-rs` or `sherpa-rs` (community, superseded). Switching to `ort` directly is
only allowed if sherpa-onnx is dropped as a whole, via a new ADR.

## Consequences

- One runtime to build, ship and update; VAD, TTS and fallback STT share it.
- We follow sherpa-onnx's model support and release cadence.
- Whistle (needle) is not ONNX and is unaffected.

## Alternatives considered

| Option | Why not |
|---|---|
| `ort` + `piper-rs` | Exact `ort` rc pin conflicts; espeak still needed; no VAD/STT wrappers |
| `ort` and sherpa-onnx together | Two ORTs in one binary (risk R8) |
| `tract-onnx` | Does not run every model we need (Piper/Kokoro) reliably |
