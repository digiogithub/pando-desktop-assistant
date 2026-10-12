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
  `GET/PUT /api/v1/config/decision-model` (provider, `effectiveBaseURL`, model, keep-alive;
  the API key is **masked**) and `GET /api/v1/decision-model/router/health`.
  `POST /api/v1/decision-model/router/test` is only a fixed "ping" health probe: **no Pando
  endpoint proxies arbitrary `/v1/systemone` questions** (gap G6, PDA-T-0165).
- Pando's consumers are off by default and **fail-open**. The assistant follows the same rule.

Principle (TypeSafe's wording): *the model classifies, the code decides.*

## Where the assistant calls it

Option A (preferred): call the same endpoint Pando is configured with, reading
`[DecisionModel]` from Pando's config via `GET /api/v1/config/decision-model`, then
`POST {effectiveBaseURL}/v1/systemone` directly. One place to configure, and the model is
already warm (`KeepAlive`). This works for local Ollama (no key) and for TypeSafe when the
key comes from `TYPESAFE_API_KEY` in the environment; a literal key stored in Pando is
masked and unusable until the upstream `POST /api/v1/decision-model/decide` proxy lands
(G6, PDA-T-0165). Without a usable endpoint the layer is fail-open (everything is
conversation).
Option B: embed `libllama` via the `llama-cpp-2` crate and implement the readout
(logits of each option token → softmax). Only if "single binary, no Ollama" becomes a hard
requirement; the readout contract differs per model, so it is a maintenance cost.

Request shape (Ollama/Jev-compatible, verified in `research/pando-api.md` §5): one shared
`state` (Pando sends a JSON-object string) plus `questions`, a **map keyed by question
name** (1–64 entries). Each question has `type` (`choice | noul | score`), `instructions`
and, for choices, an **ordered** `criteria` object of 2–26 options (order matters). The
response carries per-question `choice`, `probabilities` and `confidence`. Keep the state
small — `tev1:0.8b` has a 2050-token context and the body is capped at 64 KiB; Pando's
default timeouts are 1500 ms (Ollama) / 3000 ms (remote).

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
   - `approve/deny` map to `POST /api/v1/permissions/respond` (`action: allow | allow_session
     | deny`) or `questions/respond` for a pending item of a **session** (from
     `GET /api/v1/sessions/{id}/pending`). ACP task permissions are not visible over HTTP
     (gaps G2/G12, PDA-T-0156, PDA-T-0159), so those cells cannot be approved by voice yet.
   - `pause/resume/retry` have **no HTTP endpoint** today (only mesnada MCP tools; "resume"
     spawns a new task). They stay hidden until gap G4 lands (PDA-T-0158).
   - Items flagged `require_explicit_approval` / `never_auto_approve` are never approved
     by the confidence policy; they always need an explicit user decision.
   - `spawn` asks two more: **engine** (`pando | claude-code | cli`) and **project**.
4. **Project** (for `spawn`, and when the user says "cambia de proyecto") — choice over
   `GET /api/v1/projects` names + `current`. Also used at idle to pre-select the project the
   user is most likely talking about: state = transcript, candidates pre-filtered by an
   embedding kNN if the list is long (> 12).

5. **Voice target** ("háblale a auth", "habla con el de docs"; "vuelve", "vuelve a Pando",
   "volver") — recognised **by rule** before the intent call, like the dictation lead-ins:
   as intent classes they cost the 0.8b model 8 points of intent accuracy (96 → 88/100).
   "háblale a …" asks only the target-agent question; "vuelve" asks nothing. Both are
   reversible target switches (same as the "Hablarle" button and Esc), executed without a
   confirmation from p ≥ 0.6 (`PDA-SP-0008.R8`).

## Confidence policy (in code)

| p(top) | Action |
|---|---|
| ≥ 0.85 | execute |
| 0.6 – 0.85 | execute if reversible (pause/status/open); otherwise confirm by voice: "¿Cancelo migracion-auth?" |
| < 0.6 | treat as conversation; show the top-2 options as buttons |

Destructive actions (`cancel`, `deny`) always confirm unless p ≥ 0.95 **and** the agent id
was literally in the transcript.

p of a command is the **minimum** over the questions it depends on (intent, agent, action,
engine/project), and flagged approvals are approved only by a click — never by a spoken "sí"
([ADR 0009](adr/0009-decision-confidence-min-and-explicit-only-approvals.md)).

## Fallbacks

- Decision endpoint unreachable or slow (> 400 ms): everything is `conversation`; show a
  "decisión: no disponible" chip in the header.
- Record every decision (`transcript, questions, answers, p, executed`) to
  `~/.pando-assistant/decisions.jsonl` — this becomes the eval set for choosing models.
