---
id: PDA-SP-0008
type: spec
title: Decision layer (System One)
status: backlog
author: mcp
labels: [decide]
created: 2026-10-11T21:41:32Z
updated: 2026-10-12T14:06:03Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/decide/src/cascade.rs#Cascade
        - crates/decide/src/cascade.rs#run
        - crates/decide/src/questions.rs#intent_question
        - crates/decide/src/questions.rs#agent_question
        - crates/decide/src/questions.rs#action_question
        - crates/decide/src/questions.rs#engine_question
        - crates/decide/src/state.rs#build_state
        - crates/decide/src/protocol.rs#Request
      tests:
        - crates/decide/src/cascade.rs#tests::control_scenario_cancel_migracion_auth
        - crates/decide/src/cascade.rs#tests::spawn_asks_engine_and_project
        - crates/decide/src/cascade.rs#tests::every_request_respects_context_and_body_budget
        - crates/decide/src/protocol.rs#tests::encodes_ordered_criteria_and_null_descriptions
        - crates/decide/src/state.rs#tests::always_valid_json_within_budget
        - crates/decide/tests/live_intent.rs#intent_accuracy_above_90_percent
  R2:
    status: backlog
    trace:
      code:
        - crates/decide/src/policy.rs#policy
        - crates/decide/src/policy.rs#Thresholds
      tests:
        - crates/decide/src/policy.rs#tests::spec_table
        - crates/decide/src/policy.rs#tests::ambiguous_cancel_confirms
        - crates/decide/src/policy.rs#tests::flagged_approvals_are_explicit_only
        - crates/decide/src/policy.rs#tests::never_executes_below_threshold
        - crates/decide/src/policy.rs#tests::destructive_needs_literal_id
        - crates/decide/src/policy.rs#tests::flagged_approval_never_executes
        - crates/decide/src/policy.rs#tests::monotonic_in_confidence
  R3:
    status: backlog
    trace:
      code:
        - crates/decide/src/command.rs#to_command
        - crates/decide/src/command.rs#CommandExecutor
        - crates/decide/src/command.rs#dispatch
      tests:
        - crates/decide/src/command.rs#tests::maps_every_action
        - crates/decide/src/command.rs#tests::pause_resume_unavailable_until_g4
        - crates/decide/src/command.rs#tests::dispatch_runs_or_refuses_fast
  R4:
    status: backlog
    trace:
      code:
        - crates/decide/src/transport.rs#DecisionTransport
        - crates/decide/src/transport.rs#SystemOneClient
        - crates/decide/src/error.rs#DecisionError
        - crates/decide/src/policy.rs#Outcome::unavailable
      tests:
        - crates/decide/src/transport.rs#tests::typed_errors
        - crates/decide/src/transport.rs#tests::happy_path_sends_model_keep_alive_and_timeout
        - crates/decide/src/cascade.rs#tests::failures_are_reported_with_partial_exchanges
        - crates/decide/src/policy.rs#tests::unavailable_is_conversation
  R5:
    status: backlog
    trace:
      code:
        - crates/decide/src/log.rs#DecisionLog
        - crates/decide/src/log.rs#DecisionRecord
        - crates/decide/src/eval.rs#replay
        - crates/decide/src/eval.rs#compare_logs
        - crates/decide/src/bin/decide-eval/main.rs
      tests:
        - crates/decide/src/log.rs#tests::appends_and_rotates
        - crates/decide/src/log.rs#tests::record_round_trips_and_holds_no_secrets
        - crates/decide/src/eval.rs#tests::replay_report_is_reproducible
        - crates/decide/src/eval.rs#tests::offline_comparison_pairs_by_transcript
        - crates/decide/tests/live_intent.rs#log_and_replay_round_trip
  R6:
    status: backlog
    trace:
      code:
        - crates/decide/src/project.rs#prefilter
        - crates/decide/src/project.rs#Scorer
        - crates/decide/src/project.rs#select_project
        - crates/decide/src/questions.rs#project_question
      tests:
        - crates/decide/src/project.rs#tests::correct_project_in_top_12_at_least_98_percent
        - crates/decide/src/project.rs#tests::select_project_for_the_pill
        - crates/decide/src/cascade.rs#tests::long_lists_keep_the_mentioned_candidates
  R7:
    status: backlog
    trace:
      code:
        - crates/decide/src/questions.rs#dictation_trigger
        - crates/decide/src/policy.rs#Outcome::Dictate
      tests:
        - crates/decide/src/questions.rs#tests::dictation_triggers
        - crates/decide/src/cascade.rs#tests::dictation_lead_in_needs_no_call
  R8:
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

### PDA-SP-0008.R8 — Voice target switch ("háblale a" / "vuelve")

The decision layer SHALL recognise "háblale a <agente/célula>" (variants: "habla con el de docs", "háblale a auth") as a non-destructive SwitchTarget command that makes the named agent the voice/composer target, and "vuelve" (variants: "vuelve a Pando", "volver") as a SwitchTarget back to the primary session. Its confidence is the minimum over the questions it depends on (intent, and agent for "háblale a"). At p ≥ the reversible threshold (0.6) it SHALL execute directly without confirmation; below it the transcript goes to the conversation with the top-2 options of the weakest question as a clarification, like other commands. An unknown, `all` or missing agent SHALL never switch the target (missing-target clarification). A cell that cannot talk (CLI) or is finished SHALL refuse with a spoken reason and keep the current target.

#### Scenario: talk to an agent
- WHEN the user says "háblale a auth" with agent `migracion-auth` live and p(intent), p(agent) ≥ 0.6
- THEN the target becomes `migracion-auth` without a confirmation and the assistant says so

#### Scenario: back to the primary session
- WHEN the user says "vuelve" while targeting a cell
- THEN the target returns to the primary session (same as Esc)

#### Scenario: low confidence
- WHEN p(agent) < 0.6 for "habla con el de docs"
- THEN nothing switches and the top-2 agents are offered as a clarification

#### Scenario: unknown agent
- WHEN the model names an agent that is not live, or `all`
- THEN nothing switches and the assistant asks which agent
