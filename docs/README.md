# Pando Desktop Assistant — implementation notes

Native desktop assistant for Pando: a voice-first "living organism" UI, written in Rust with
GPUI, that talks to a running Pando instance over its HTTP API and lets you talk to — and
steer — every agent mesnada coordinates.

These notes were written before any code exists. They capture the decisions, the research and
the facts about the surrounding systems that an implementer (human or Claude Code) needs.
Verify anything marked **[verify]** against the current Pando source in `../../pando` before
relying on it.

| Doc | What it covers |
|---|---|
| [01-strategy.md](01-strategy.md) | Goals, stack, crate layout, milestones, risks |
| [02-voice-pipeline.md](02-voice-pipeline.md) | Mic capture → Whistle STT (C FFI) → TTS (ONNX) → playback, barge-in |
| [03-decision-layer.md](03-decision-layer.md) | Jev / System One decision models: command vs. chat, which agent, which project |
| [04-pando-api.md](04-pando-api.md) | The Pando endpoints the assistant uses (sessions, streams, orchestrator, instances) |
| [05-claude-code-interop.md](05-claude-code-interop.md) | Talking to Claude Code sessions: ACP, cross-session messaging, channels, hooks |
| [06-ui-mapping.md](06-ui-mapping.md) | How API events drive the organism (states, cells, engines, statuses) |

Design system: [`../design-system/README.md`](../design-system/README.md) and
[`../design-system/tokens.json`](../design-system/tokens.json). Open
`../design-system/reference/organism.html` in a browser to see the target behaviour.

Language: code, comments and docs in English. UI copy is Spanish first (`es`), English second.
