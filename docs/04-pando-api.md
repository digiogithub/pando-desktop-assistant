# 04 · Pando API surface used by the assistant

> Reconciled on 2026-10-12 (PDA-T-0016) with [`research/pando-api.md`](research/pando-api.md),
> which was read from Pando at commit `839540537` and is the **authoritative reference**
> (full endpoint inventory, payloads, file:line evidence, gaps G1–G17). This note is the
> short version. Protocol choice: REST + `chat/stream` SSE, not AG-UI
> ([ADR 0002](adr/0002-rest-chat-stream-not-ag-ui.md)).

## Servers, discovery and auth

- Who serves the API: `pando serve` and `pando app` (**https**, auto-generated local CA at
  `~/.config/pando/tls/ca.crt` — trust it in rustls, never disable verification) and
  `pando desktop` (**http**). A plain `pando` TUI or `pando acp` serves **no HTTP API**.
  Default port 8765, fallback 8766..8775, then random.
- Discovery: `/tmp/pando-instances/<instance_id>.json` (one per live process; no scheme, no
  token) → keep `web_port > 0` → probe `GET /health` (unauthenticated; https first for
  `webui`, http for `desktop`) → fallback `https://localhost:8765`, then `http://…`.
  The hard-coded `/tmp` path is not portable (gap G13, PDA-T-0168).
- **Token**: ephemeral, 32 random bytes generated at each server start, **not persisted**.
  Get it from `GET`/`POST /api/v1/token` → `{"token":"…"}` (no credentials on a loopback
  bind; Basic auth otherwise). Send it as **`X-Pando-Token`** (or `?token=` for
  EventSource). `Authorization: Bearer` only works on the AG-UI routes. Refetch on 401
  (server restarts change token and maybe port). Send `X-Pando-Client: pando-assistant`.
- **Never call** `POST /api/v1/config/api-server/regenerate-token`: it rotates in memory
  and logs out every other client (gap G15, PDA-T-0169).
- No API/protocol version: `/api/v1/version` is the build version only → gate by probing
  (gap G10, PDA-T-0163).

## Discovery endpoints

| Endpoint | Use |
|---|---|
| `GET /health`, `GET /api/v1/version` | liveness, startup mode, build version |
| `GET /api/v1/project`, `/project/context` | current project (`{cwd, version}`) |
| `GET /api/v1/projects`, `GET /api/v1/projects/events` (SSE) | registered projects (the "which project" list); child project APIs are reachable through the parent at `/api/v1/projects/{id}/web/api/v1/…` |
| `GET /api/v1/instances` | running Pando instances (`instance_id, path, pid, web_port, mode, parent_instance_id, is_primary, …`) |
| `GET /api/v1/models`, `PUT /api/v1/models/active` | header chip; **always pass `sessionId`** to `PUT` — without it the global coder model in config changes |

## Conversation (the "talk to mesnada" path)

| Endpoint | Use |
|---|---|
| `GET /api/v1/sessions?limit=&offset=` | list (newest first, paginated, adds `is_running`, `run_seq`) |
| `GET /api/v1/sessions/{id}` | `{session, messages, is_running, run_seq}` (PascalCase keys, untagged parts) |
| `POST /api/v1/chat/stream` | `{sessionId?, prompt, model?}` → SSE. **There is no `POST /sessions`**: an empty `sessionId` creates the session and its id arrives in the first `session` event (gap G8, PDA-T-0163). If the session is busy, the prompt becomes steering |
| `GET /api/v1/sessions/{id}/stream` | reattach: replay of the last 500 events (kept 10 min) + live |
| `POST /api/v1/sessions/{id}/steer` | `{prompt}` → 202, or 409 `not_busy` — voice "y además…" |
| `POST /api/v1/sessions/{id}/cancel` | stop |
| `GET /api/v1/sessions/{id}/pending` | `{permissions, questions, running, run_seq}`; also the run heartbeat (poll ≈4 s) |
| `POST /api/v1/permissions/respond` | `{id, sessionId, action: allow \| allow_session \| deny}` |
| `POST /api/v1/questions/respond` | `{id, sessionId, answers:[{questionId, selected, otherText}], cancelled?}` |
| `GET/POST /api/v1/sessions/{id}/auto-approve` | toggle |

SSE wire format: `event: <name>\ndata: <one-line JSON>\n\n`; **no ids, no retry, no
keep-alive, no `Last-Event-ID`** (gap G9, PDA-T-0161). Events, in typical order:
`session`, `run`, `permission_request`, `question_request`, `thinking_delta`,
`content_delta`, `tool_call`, `tool_call_update`, `tool_result`, `plan_update`,
`todos_update`, `token_usage`, `system_message`, `steering_queued`, `steering_injected`,
`conclusion_queued`, `conclusion_injected`, `resurrected`, `goal_status`, `error`, `done`.

- There is **no `response` event** on the wire: build the final text by accumulating
  `content_delta` until `done` (a `message_complete` event is gap G14, PDA-T-0162).
- If the body closes **without `done`**, the run may still be alive → reattach.
- Slash-command streams end with `session {running:false}` and have no `done`.
- `conclusion_*` and `resurrected` carry only a **human note** (`{session_id, message}`),
  no task id, summary or status (gap G3, PDA-T-0157).

Reattach (`run_seq` protocol, as in the web UI; task PDA-T-0072): on load set
`seenRunSeq = is_running ? -1 : run_seq`; poll `/pending`; if `run_seq > seenRunSeq` and no
stream is open, open `/sessions/{id}/stream` and **reset the live bubble** before applying
the replay. This is also how resurrected runs (below) are discovered until a global
session-activity stream exists (gap G5, PDA-T-0160).

Organism mapping: `thinking_delta` → `thinking`; first `content_delta` → stop thinking;
`tool_call` → nucleus satellite blip; `done` with accumulated text → speak; `conclusion_*`
→ wave from the body (the concluding cell is unknown until G3); `resurrected` → body bulge.

## Agents = orchestrator tasks (mesnada)

| Endpoint | Use |
|---|---|
| `GET /api/v1/orchestrator/tasks` | the cell list (poll ≈1 s and diff; push events are gap G1, PDA-T-0155) |
| `POST /api/v1/orchestrator/tasks` | spawn: `{prompt, work_dir?, model?, engine?, tags?, dependencies?, background}` |
| `GET /api/v1/orchestrator/tasks/{id}` | detail for the side card |
| `POST /api/v1/orchestrator/tasks/{id}/cancel`, `DELETE …/{id}` | voice "cancela" |
| `GET /api/v1/orchestrator/delegation/metrics` | telemetry chip (optional) |

The HTTP API returns a **projection, `TaskResponse`**: `id, name, prompt, agent (= engine),
model, persona, status, progress (int %), tokens (always 0), output, current_tool,
tool_calls, created_at, updated_at`. It **omits** `work_dir, tags, error, exit_code,
output_tail, acp_session_id, acp_status, parent_session_id, project_*, conclusion,
dependencies` (gap G2, PDA-T-0156).

`status` ∈ `pending | running | paused | completed | failed | cancelled`.
Engine (`agent`) ∈ `copilot | claude | gemini | opencode | ollama-claude | ollama-opencode |
mistral | acp | acp-claude | acp-codex | acp-custom | acp-server | pando | warm-acp` plus
custom engines from `.pando/mesnada/engines/*.template.yaml`.

**There is no HTTP pause or resume** (confirmed). `Orchestrator.Pause/Resume` exist only as
mesnada MCP tools, and "resume" spawns a new task with a new prompt. The voice actions
`pause`/`resume` are hidden until gap G4 lands (PDA-T-0158).

## Conclusions and delegated results (what gets spoken)

Subagents end with a sentinel block; the **last** block wins and the body is YAML:

```
<pando:conclusion>
status: success         # success | partial | failed | blocked
summary: API estable, sin regresiones
artifacts: [internal/api/handlers_chat.go]
memory_refs: []
follow_up: ""
confidence: 0.9
</pando:conclusion>
```

(`done` is not a valid status: it normalises to empty and Pando fills it from the task's
terminal state.) Pando enriches the conclusion and injects it into the **parent session**
as text (`FormatForParent`), visible in `GET /sessions/{id}` history, **not in SSE**:

```
[delegated-result task=<id> engine=<e> project=<p> status=<success|partial|failed|blocked> confidence=0.90]
summary: …
follow_up: …
artifacts: …
```

If the parent run is active, SSE shows `conclusion_queued` → `conclusion_injected` and the
model reacts in the same stream; if it is idle, Pando starts a **new background run** with
no client attached (first replayed event `resurrected`), discovered via `run_seq`.
The assistant speaks the parent's reply, or the `summary` (+ `follow_up`) parsed from the
`[delegated-result …]` block (parser task PDA-T-0053). The structured conclusion is not
reachable over HTTP (gap G3, PDA-T-0157). Flow details: PDA-T-0028.

## Events

| Endpoint | Use |
|---|---|
| `GET /api/v1/notifications/stream` (SSE, `event: notification`) | **user-facing toasts only** (`{id,time,level,source,message,ttl}`: LLM retries, tool errors, LSP). It carries **no task lifecycle and no permission requests** |
| `GET /api/v1/config/events` (SSE) | config changes (decision model switched, etc.) |
| `GET /api/v1/logs/stream` | optional debug pane |

All of these have no replay; reconnect with exponential backoff (1 s doubling to 30 s).

## Decision model

`GET/PUT /api/v1/config/decision-model` (API key masked), `GET /api/v1/decision-model/router/health`
(availability chip). `POST /api/v1/decision-model/router/test` is a fixed ping probe, not a
question proxy. See 03.

## Other protocols Pando speaks (not used in v1, good to know)

- **ACP** (`pando acp`, stdio) — Pando itself can be an ACP server for editors; mesnada also
  *consumes* ACP engines (`acp-claude`, `acp-codex`) and can reuse warm per-project ACP
  instances (`docs/delegation.md` in Pando).
- **AG-UI** (`/api/v1/agui`, Bearer) — what Bitácora uses; not used here (ADR 0002).
- **IPC bus** on localhost between instances ("hot-peer delegation", `instance.ping`).
- The official SDKs (TS/Python/Java/.NET HTTP clients) send `Authorization: Bearer` and
  call endpoints that do not exist: **do not use them as a reference**.
