# 03 · Decision layer (Jev / System One)

## What a decision model is

A decision model (Jev-style, "System One") answers typed questions — choice, score, yes/no —
with a probability distribution in one forward pass. It does not generate text. Pando already
uses one through `POST {base}/v1/systemone` (see `../../pando/docs/decision-model.md`):

- Providers: **Ollama ≥ 0.35** with a model that has the `decision` capability
  (`ollama pull tev1:0.8b`), **TypeSafe Jev** hosted (`jev-latest`), or a custom gateway.
  llama.cpp ≥ 0.6.0 also serves `/v1/systemone` (models: laya, julia-1, lev, openjev, kev,
  nimble) and can be used as the custom gateway.
- Pando config lives in the top-level `[DecisionModel]` block; the API exposes
  `/api/v1/config/decision-model` and `POST /api/v1/decision-model/router/test`.
- Pando's consumers are off by default and **fail-open**. The assistant follows the same rule.

Principle (TypeSafe's wording): *the model classifies, the code decides.*

## Where the assistant calls it

Option A (preferred): call the same endpoint Pando is configured with, reading
`[DecisionModel]` from Pando's config via `GET /api/v1/config/decision-model`. One place to
configure, and the model is already warm (`KeepAlive`).
Option B: embed `libllama` via the `llama-cpp-2` crate and implement the readout
(logits of each option token → softmax). Only if "single binary, no Ollama" becomes a hard
requirement; the readout contract differs per model, so it is a maintenance cost.

Request shape (Ollama/Jev-compatible): one shared `state` string plus a list of typed
`questions`. Keep the state small — `tev1:0.8b` has a 2050-token context and Pando caps the
body at 64 KiB.

## The questions, in order

Every transcript goes through this cascade. Each step is a separate question so confidences
stay interpretable; `state` = transcript + the live list of agents and projects.

1. **Intent** — choice: `control | conversation | dictation`.
   - `control` → steps 2–3, then execute in code, never through the LLM.
   - `conversation` → send to mesnada (chat stream), speak the conclusion.
   - `dictation` → insert text into the selected agent's prompt box, do not send.
2. **Target agent** (only for control) — choice over the current agent ids + `all | none`.
   Keyword biasing in Whistle makes ids transcribe well; still, include display names.
3. **Action** — choice: `pause | resume | cancel | status | approve | deny | retry | open | spawn`.
   - `approve/deny` map to `POST /api/v1/permissions/respond` or `questions/respond` for the
     agent's pending item.
   - `spawn` asks two more: **engine** (`pando | claude-code | cli`) and **project**.
4. **Project** (for `spawn`, and when the user says "cambia de proyecto") — choice over
   `GET /api/v1/projects` names + `current`. Also used at idle to pre-select the project the
   user is most likely talking about: state = transcript, candidates pre-filtered by an
   embedding kNN if the list is long (> 12).

## Confidence policy (in code)

| p(top) | Action |
|---|---|
| ≥ 0.85 | execute |
| 0.6 – 0.85 | execute if reversible (pause/status/open); otherwise confirm by voice: "¿Cancelo migracion-auth?" |
| < 0.6 | treat as conversation; show the top-2 options as buttons |

Destructive actions (`cancel`, `deny`) always confirm unless p ≥ 0.95 **and** the agent id
was literally in the transcript.

## Fallbacks

- Decision endpoint unreachable or slow (> 400 ms): everything is `conversation`; show a
  "decisión: no disponible" chip in the header.
- Record every decision (`transcript, questions, answers, p, executed`) to
  `~/.pando-assistant/decisions.jsonl` — this becomes the eval set for choosing models.
