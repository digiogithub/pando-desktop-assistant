---
id: PDA-SP-0005
type: spec
title: Agents as cells
status: backlog
author: mcp
labels: [organism, pando-client]
created: 2026-10-11T21:41:32Z
updated: 2026-10-11T22:03:14Z
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
Adapter contract between agent sources (mesnada tasks, Pando instances, Claude Code sessions) and `organism::Inputs`, plus the actions available on each cell.

## Scope
Engine/status mapping (`docs/06-ui-mapping.md`), stable order on the ring, cell side card, per-engine actions, voice target ("háblale"/"vuelve").

## Requirements

### PDA-SP-0005.R1 — Mapping mesnada tasks to cells

Each orchestrator task SHALL map to a cell: id = `task.id`, label = `tags[0]` or first 24 chars of `prompt`/`name`; engine `pando` → Pando, `acp-claude`/`acp-*`/`warm-acp` → ClaudeAcp, text engines (`claude`, `copilot`, `gemini`, `opencode`, `mistral`, custom text) → Cli; status `pending` → Working dimmed 60 %, `running` → Working, `paused` → Paused, `completed` → Done, `failed` → Error, `cancelled` → Done then fade after 10 s. Waiting SHALL override when the task or its session has a pending permission/question.

#### Scenario: failed task
- GIVEN a task moves to `failed`
- THEN its cell shows the error nucleus and `!` badge

### PDA-SP-0005.R2 — Task source: polling or stream

The cell source SHALL poll `GET /api/v1/orchestrator/tasks` every 1 s while the window is visible (5 s when hidden) and diff by id, and SHALL switch to an orchestrator event stream when Pando exposes one (gap G1). Task creation observed SHALL push a Delegating overlay to the body.

#### Scenario: spawn
- WHEN mesnada spawns a subagent
- THEN a cell buds (mitosis) within 1.5 s (polling) and the body shows Delegating

### PDA-SP-0005.R3 — Pando instances as cells

Warm per-project Pando instances (`GET /api/v1/instances`, `is_primary == false`) SHALL appear as Pando cells labelled with their project, Working when they have in-flight runs, Done otherwise.

#### Scenario: project child started
- WHEN a project child server starts
- THEN a Pando cell labelled with the project appears

### PDA-SP-0005.R4 — Side card and per-engine actions

Selecting a cell SHALL show label, engine, status, project (work_dir matched to projects when available), last 3 lines of output, current tool and conclusion status/confidence when available, and the actions allowed by engine: Pando/ClaudeAcp → Hablarle, Cancelar, Ver conclusión; waiting → Aprobar/Denegar; ClaudeExternal → Enviar mensaje, Avísame al terminar; Cli → Ver salida completa, Relanzar. Pause/Resume SHALL be hidden until Pando exposes them (gap G4).

#### Scenario: relaunch CLI task
- WHEN the user clicks Relanzar
- THEN `POST /orchestrator/tasks` is sent with the same prompt and engine

### PDA-SP-0005.R5 — Voice target

Voice prompts SHALL go to mesnada (the primary session) unless the user selected a talkable cell and said "háblale" or clicked Hablarle; then prompts go to that agent's session until "vuelve" or Esc. The header pill SHALL always show the current target.

#### Scenario: return to mesnada
- GIVEN the target is `migracion-auth`
- WHEN the user says "vuelve"
- THEN the pill shows "mesnada" and the next prompt goes to the primary session

### PDA-SP-0005.R6 — What is spoken and when

Speech priority SHALL be: conclusion summary (+ follow_up) of the cell the user asked about > mesnada's final reply text > Claude hook Stop summary when the user asked to be notified. Tool output SHALL never be spoken. While the body is Listening or Speaking new conclusions SHALL wait in a queue and their cells pulse once. Delegated results SHALL be parsed from the `[delegated-result task=… status=… confidence=…]` block in history until Pando exposes structured conclusions (gap G3).

#### Scenario: conclusion during speech
- GIVEN the assistant is speaking
- WHEN a subagent concludes
- THEN its cell pulses and the summary is spoken after the current utterance
