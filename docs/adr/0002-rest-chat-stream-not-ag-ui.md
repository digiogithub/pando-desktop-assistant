# 0002 · Talk to Pando over REST + `chat/stream` SSE, not AG-UI

- **Status:** accepted
- **Date:** 2026-10-12
- **Deciders:** project owner, planning agents
- **Refs:** PDA-T-0044, PDA-T-0050, PDA-T-0072, [`docs/research/pando-api.md`](../research/pando-api.md) §1, §3, §6; [`docs/research/bitacora-gpui.md`](../research/bitacora-gpui.md) §0, §2

## Context

Pando exposes two chat protocols: its own REST API (`POST /api/v1/chat/stream`, SSE,
`X-Pando-Token` header) and an AG-UI adapter (`/api/v1/agui`, `Authorization: Bearer`).
Bitácora's reusable Rust SDK (`pando-rs`) speaks AG-UI for chat. The assistant, however,
needs much more than chat: orchestrator tasks (cells), sessions, steering, cancel,
pending permissions/questions, `run_seq` reattach, models, projects and instances. Those
exist only on the REST API, and the web UI (the reference client) uses REST for all of it,
including the chat stream and its richer event set (`plan_update`, `goal_status`,
`permission_request`, `question_request`, `done`, …).

## Decision

The assistant uses the Pando REST API for everything, including chat through
`POST /api/v1/chat/stream` and reattach through `GET /api/v1/sessions/{id}/stream`. Auth is
the ephemeral token from `GET /api/v1/token` sent as `X-Pando-Token`. From `pando-rs` we
reuse only the transport core (client, config, errors, SSE parser), not its AG-UI types.

## Consequences

- One auth scheme, one event vocabulary, parity with the web UI.
- We write our own typed events and the chat reducer (the AG-UI reducer is not reused).
- Missing REST features become upstream tasks (gaps G1–G17, `PDA-T-0155`…`PDA-T-0170`),
  and the assistant keeps working, degraded, until they land.

## Alternatives considered

| Option | Why not |
|---|---|
| AG-UI for chat + REST for the rest | Two auth schemes and two event models for one session; AG-UI lacks steering, `run_seq`, plan/goal events |
| AG-UI only | No orchestrator, pending or project endpoints |
