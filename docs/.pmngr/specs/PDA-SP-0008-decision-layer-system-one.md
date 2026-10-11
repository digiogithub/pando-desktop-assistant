---
id: PDA-SP-0008
type: spec
title: Decision layer (System One)
status: backlog
author: mcp
labels: [decide]
created: 2026-10-11T21:41:32Z
updated: 2026-10-11T22:04:13Z
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
---

## Purpose
"The model classifies, the code decides": each transcript goes through typed questions to a decision model, and the code executes commands safely.

## Scope
Crate `decide` (no UI or own network except the transport trait): intent/agent/action/project cascade, confidence policy, confirmations, fail-open, `decisions.jsonl` log. Transport: `{effectiveBaseURL}/v1/systemone` read from `/api/v1/config/decision-model` (or G6 proxy when it exists).

Refs: `docs/03-decision-layer.md`, `docs/research/pando-api.md` §5.

## Requirements

### PDA-SP-0008.R1 — Cascade of typed questions

Each transcript SHALL be classified by separate questions over a small shared `state` (transcript + live agents + projects, within the model context and 64 KiB): 1) intent `control | conversation | dictation`; 2) target agent (control only) over current ids + display names + `all | none`; 3) action `pause | resume | cancel | status | approve | deny | retry | open | spawn`; 4) for spawn: engine `pando | claude-code | cli` and project. Requests SHALL follow the `/v1/systemone` shape (questions keyed by name, ordered criteria, types choice/noul/score).

#### Scenario: control
- GIVEN "cancela migración auth"
- THEN intent=control, agent=`migracion-auth`, action=cancel, each with its probability

### PDA-SP-0008.R2 — Confidence policy in code

Execution SHALL follow: p(top) ≥ 0.85 execute; 0.6–0.85 execute only if reversible (pause/status/open), otherwise confirm by voice ("¿Cancelo migracion-auth?"); < 0.6 treat as conversation and show top-2 options as buttons. Destructive actions (`cancel`, `deny`) SHALL always confirm unless p ≥ 0.95 and the agent id literally appears in the transcript.

#### Scenario: ambiguous cancel
- GIVEN action=cancel with p 0.9 and the id not literally present
- THEN the assistant asks for confirmation before calling the API

### PDA-SP-0008.R3 — Command execution without an LLM

Control decisions SHALL execute directly against the Pando API (cancel → `/orchestrator/tasks/{id}/cancel`, approve/deny → `/permissions/respond`, status → spoken summary from task data, open → select cell, spawn → `POST /orchestrator/tasks`, retry → relaunch), never by prompting an LLM. Unsupported actions (pause/resume until G4) SHALL be answered "no disponible" instead of failing silently.

#### Scenario: status
- WHEN the user asks "¿cómo va bitácora?"
- THEN the assistant speaks status and progress from the task data in < 500 ms

### PDA-SP-0008.R4 — Fail-open and transport

The decision transport SHALL read provider, `effectiveBaseURL`, model and keepAlive from `GET /api/v1/config/decision-model`, show health from `/decision-model/router/health`, call `/v1/systemone` directly (key from env for TypeSafe) or a Pando proxy when it exists (gap G6), and treat unreachable, timeout (> 400 ms) or malformed answers as `conversation`, showing a "decisión: no disponible" chip.

#### Scenario: Ollama down
- WHEN the decision call times out
- THEN the transcript is sent as conversation and the chip turns grey

### PDA-SP-0008.R5 — Decision log

Every decision SHALL be appended to `<data_dir>/decisions.jsonl` with transcript, state hash, questions, answers with probabilities, latency, policy outcome and whether it executed, rotated at 10 MB. A CLI `pando-assistant decisions eval <file>` SHALL replay it against a configured model to compare models.

#### Scenario: eval
- WHEN running eval with a different model
- THEN a report lists agreement and confidence deltas per question

### PDA-SP-0008.R6 — Project selection

Project choice SHALL be a question over `GET /api/v1/projects` names + `current`, pre-filtered to ≤ 12 candidates by fuzzy/embedding kNN when the list is longer, used for spawn and "cambia de proyecto", and shown in the header pill with its confidence.

#### Scenario: switch
- WHEN the user says "cambia a bitácora"
- THEN the pill shows `proyecto · bitacora · 0.93`

### PDA-SP-0008.R7 — Dictation

Intent `dictation` SHALL insert the transcript into the composer of the current voice target without sending it, and the UI SHALL indicate that dictation was applied.

#### Scenario: dictate
- WHEN the user says "dicta: añade un test para el parser"
- THEN the composer contains the text and nothing is sent
