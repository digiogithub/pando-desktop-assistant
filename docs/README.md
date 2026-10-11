# Pando Desktop Assistant — implementation notes

Native desktop assistant for Pando: a voice-first "living organism" UI, written in Rust with
GPUI, that talks to a running Pando instance over its HTTP API and lets you talk to — and
steer — every agent mesnada coordinates.

These notes were written before any code exists. They capture the decisions, the research and
the facts about the surrounding systems that an implementer (human or Claude Code) needs.
They were reconciled with the verified research in [`research/`](research/) on 2026-10-12
(PDA-T-0016): no **[verify]** is left open in `01`–`06`; what is still unknown is linked to
a spike or an upstream gap task. Where a note and `research/*.md` disagree, the research wins.

| Research | What it covers |
|---|---|
| [research/pando-api.md](research/pando-api.md) | Authoritative Pando API reference, SSE protocol, gaps G1–G17 |
| [research/voice-stack.md](research/voice-stack.md) | STT, VAD, wake word, TTS, audio, hotkeys, model distribution |
| [research/bitacora-gpui.md](research/bitacora-gpui.md) | GPUI/GPUI Kit patterns and reusable code from Bitácora |
| [research/organism-port.md](research/organism-port.md) | Porting the organism to Rust + GPUI |

| Doc | What it covers |
|---|---|
| [01-strategy.md](01-strategy.md) | Goals, stack, crate layout, milestones, risks |
| [02-voice-pipeline.md](02-voice-pipeline.md) | Mic capture → `pando-voice` worker (Whistle STT, sherpa-onnx VAD/TTS) → playback, barge-in |
| [03-decision-layer.md](03-decision-layer.md) | Jev / System One decision models: command vs. chat, which agent, which project |
| [04-pando-api.md](04-pando-api.md) | The Pando endpoints the assistant uses (auth, chat stream, orchestrator, conclusions, events) |
| [05-claude-code-interop.md](05-claude-code-interop.md) | Talking to Claude Code sessions: ACP, cross-session messaging, channels, hooks |
| [06-ui-mapping.md](06-ui-mapping.md) | How API events drive the organism (states, cells, engines, statuses) |

Architecture decisions: [`adr/`](adr/README.md) (one ADR per decision, template in
[`adr/0000-template.md`](adr/0000-template.md)):

| ADR | Decision |
|---|---|
| [0001](adr/0001-gpui-kit-pin-and-ui-facade.md) | GPUI only through `gpui-kit =0.7.1`, behind the `ui/` facade |
| [0002](adr/0002-rest-chat-stream-not-ag-ui.md) | REST + `chat/stream` SSE, not AG-UI |
| [0003](adr/0003-rust-i18n.md) | `rust-i18n` YAML locales, Spanish first |
| [0004](adr/0004-voice-worker-separate-process.md) | Voice runs in a separate `pando-voice` worker process |
| [0005](adr/0005-sherpa-onnx-single-onnx-runtime.md) | `sherpa-onnx` is the only ONNX Runtime |
| [0006](adr/0006-tiny-skia-tests-and-fallback-only.md) | GPUI paths for the organism; `tiny-skia` for tests/fallback only |

Design system: [`../design-system/README.md`](../design-system/README.md) and
[`../design-system/tokens.json`](../design-system/tokens.json). Open
`../design-system/reference/organism.html` in a browser to see the target behaviour.

Language: code, comments and docs in English. UI copy is Spanish first (`es`), English second.
