# Architecture Decision Records

One file per decision, `NNNN-<slug>.md`, numbered in order and never renumbered. Copy
[`0000-template.md`](0000-template.md) to add one. An ADR is immutable once accepted: to
change a decision, write a new ADR and mark the old one `superseded by NNNN`.

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-gpui-kit-pin-and-ui-facade.md) | GPUI only through `gpui-kit =0.7.1`, behind the `app/src/ui/` facade | accepted |
| [0002](0002-rest-chat-stream-not-ag-ui.md) | Talk to Pando over REST + `POST /api/v1/chat/stream` (SSE), not AG-UI | accepted |
| [0003](0003-rust-i18n.md) | UI strings with `rust-i18n` YAML locales, Spanish first | accepted |
| [0004](0004-voice-worker-separate-process.md) | STT/TTS/VAD run in a separate `pando-voice` worker process | accepted; partially superseded by 0007 |
| [0005](0005-sherpa-onnx-single-onnx-runtime.md) | `sherpa-onnx` is the only ONNX Runtime provider | accepted |
| [0006](0006-tiny-skia-tests-and-fallback-only.md) | Organism renders with GPUI paths; `tiny-skia` only for tests and fallback | accepted |
| [0007](0007-espeak-ng-gpl-isolated-tts-worker.md) | espeak-ng (GPL-3) only in a dedicated `pando-tts` process; STT in a permissive `pando-stt` worker | accepted |
| [0008](0008-stt-engines-whistle-default-parakeet-optional.md) | Whistle is the default STT for PTT commands (≤ 5 s segments, keyword bias); Parakeet v3 optional; Whisper dropped | accepted |
