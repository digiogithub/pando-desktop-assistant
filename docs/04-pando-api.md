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
reachable over HTTP until G2 lands (gap G3, PDA-T-0157, folded into G2). Flow details below.

### Delegation → conclusion → resurrection flow (PDA-T-0028)

Ground truth: Pando `docs/delegation.md`, `internal/llm/agent/agent.go` (event types and the
human notes at `:708`/`:757`), `internal/api/handlers_chat.go` (`conclusion_*`/`resurrected`
SSE writers, payload `{session_id, message}` only), `internal/app/delegation_supervisor.go`
(`enqueueResurrection`), `internal/api/resume_run.go` (server-owned background run) and
`internal/mesnada/conclusion/{parse,format}.go`. Delegation is **default-off** in Pando
(`Mesnada → Delegation`).

```mermaid
sequenceDiagram
    autonumber
    participant A as Assistant (pando-client)
    participant P as Pando API (parent session)
    participant L as Parent agent loop
    participant O as Mesnada orchestrator
    participant S as Subagent task

    A->>P: POST /chat/stream {sessionId, prompt}
    P-->>A: SSE session, run{runSeq:n}, content_delta …
    L->>O: tool_call mesnada_spawn_agent / spawn_agent
    P-->>A: SSE tool_call, tool_result (task id inside content)
    O->>S: start task (cold CLI, warm ACP or hot peer)
    loop every 1–2 s
        A->>P: GET /orchestrator/tasks
        P-->>A: TaskResponse list (status, progress; G2 adds parent_session_id, conclusion)
    end
    S-->>O: output ends with <pando:conclusion> YAML (last block wins)
    O->>O: parse + enrich (task id, engine, project, parent), durable events.jsonl
    alt Case A: parent run still active
        O->>L: queue conclusion
        P-->>A: SSE conclusion_queued {session_id, message}
        L->>L: inject [delegated-result …] at next step
        P-->>A: SSE conclusion_injected, then the model's reaction (content_delta …, done)
    else Case B: parent idle (resurrection)
        O->>P: delegation supervisor starts a new background run (no client attached)
        Note over P: run_seq = n+1, is_running = true
        loop every ~4 s
            A->>P: GET /sessions/{id}/pending
            P-->>A: {running, run_seq: n+1}
        end
        A->>A: run_seq > seen and no stream open → claim, reset live bubble
        A->>P: GET /sessions/{id}/stream (replay ≤500 events / 10 min + live)
        P-->>A: SSE session, run{runSeq:n+1}, resurrected, content_delta …, done
    end
    A->>P: GET /sessions/{id} (history)
    P-->>A: messages incl. the injected [delegated-result task=… status=… confidence=…] text
```

| Step | What the wire shows | Client code that handles it |
|---|---|---|
| Spawn (1–4) | `tool_call`/`tool_result` for the mesnada spawn tool; task id only inside `tool_result.content` | `ChatEvent::{ToolCall,ToolResult}` in `crates/pando-client/src/chat.rs`, folded by `ChatModel` in `model.rs` |
| Task lifecycle (5–7) | No push stream (G1): poll `GET /orchestrator/tasks` | `PandoClient::list_tasks` + `TaskTracker::update` → `TaskChange::{Appeared,Updated,Finished,Removed}` in `crates/pando-client/src/api.rs` |
| Conclusion block (8–9) | `<pando:conclusion>` in the subagent output; structured `conclusion` on the task only with G2 | `parse_conclusion` / `Conclusion` in `crates/pando-client/src/conclusion.rs`; `TaskResponse.conclusion` (`Option`, additive) in `api.rs` |
| Case A (10–13) | `conclusion_queued` → `conclusion_injected`; human note only, no task id | `ChatEvent::{ConclusionQueued,ConclusionInjected}(Note)` → `NoticeKind::{ConclusionQueued,ConclusionInjected}` rows in `ChatModel::apply` (`model.rs`) |
| Case B (14–20) | nothing on any open stream; only `run_seq` grows | `follow_session` / `seen_for` / `should_attach` in `crates/pando-client/src/follow.rs`: poll `/pending`, `FollowEvent::ResetBubble`, attach `/stream`, `FollowEvent::HistoryReload` if the replay was empty, reattach if the body closes without `done` |
| `resurrected` (20) | first event after `run` in the replay | `ChatEvent::Resurrected` → `run_active = true` + `NoticeKind::Resurrected` notice (`model.rs`) |
| History (21–22) | `[delegated-result …]` + `summary:`/`follow_up:`/`artifacts:` as message text, never in SSE | `parse_delegated_results` / `DelegatedResult` (`conclusion.rs`), turned into `ItemKind::Conclusion` rows by `ChatModel::load_history` (`model.rs`) |

Notes:

- **Who owns a resurrected run.** Pando streams it to whichever surface has the session open;
  with nothing attached it still runs and is persisted. The assistant therefore must keep a
  `SessionFollow` alive for every session it shows (and call `begin_local_run`/`observe`
  around its own runs so the poller does not double-attach). A prompt sent while a resurrected
  run is active becomes steering for it; cancelling the session stops it.
- **Stale conclusions.** A conclusion replayed from the durable log after more than an hour is
  acknowledged without resurrecting the parent, so a restart does not wake old sessions.
- **What gets spoken.** Prefer the parent's reply (`content_delta` accumulated to `done` of the
  resurrected run). Fall back to `summary` (+ `follow_up`) from the parsed `[delegated-result]`
  block. Once G2 is deployed, `TaskResponse.conclusion` of a `Finished` task gives the same
  data without a history reload and `parent_session_id` maps the cell to its parent session.
- **Upstream G2** (`PDA-T-0156`): implemented in Pando on branch
  `task/PDA-T-0156-task-response-fields` (commit `0f00fd924`, additive `omitempty` fields incl.
  `parent_session_id`, `project_*`, `acp_*`, `conclusion`), **pending owner review**, not on
  Pando `main`. The client already parses those fields as optional, so older servers keep
  working. G3's event half (task id/summary in `conclusion_*`/`resurrected`) is not planned;
  polling sees the terminal status (`docs/research/upstream-gap-review.md`). G5 (a global
  session-activity stream) would replace the `/pending` polling for Case B.
- **Fixture review.** `session_reattach.sse` (real, Pando v1.3.0) confirms the replay shape
  `session` → `goal_status` → `run{runSeq}` → events → `done` that the follower relies on.
  `conclusion_queued`, `conclusion_injected` and `resurrected` are only covered by the
  synthetic fixture `synthetic_events.sse` and by `tests/follow.rs`
  (`resurrected_run_is_attached_after_run_seq_grows`); their payload shape
  `{session_id, message}` matches `handlers_chat.go`. A live delegation capture is still
  pending (PDA-T-0027).

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
