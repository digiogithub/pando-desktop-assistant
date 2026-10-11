# Upstream gap review: what Pando already covers (G1–G17)

- **Date:** 2026-10-12
- **Scope:** gaps G1–G17 in [`pando-api.md`](pando-api.md) §6.2, planned as tasks
  `PDA-T-0155`..`PDA-T-0170` (epic `PDA-EP-0015`).
- **Constraint from the product owner:** Pando already has an AG-UI API and the WebUI
  HTTP+SSE API, which should be enough. Anything added to Pando must be **generic** and
  limited to what is **indispensable** for the assistant.
- **Method:** read the Pando source at `/www/MCP/Pando/pando`: `internal/api/routes.go`,
  `internal/agui/*`, `internal/mesnada/server/*`, `internal/ipc/*` and the WebUI client.
  Each conclusion below cites the file and symbol it rests on.
- **Related:** [ADR 0002](../adr/0002-rest-chat-stream-not-ag-ui.md) (REST + `chat/stream`,
  not AG-UI), [[pando-assistant/plans/backlog-execution_plan.md]].

## Summary

| Gap | Task | Class | One line |
|---|---|---|---|
| G1 task push events | PDA-T-0155 | NOT-NEEDED | Poll `GET /api/v1/orchestrator/tasks` every 1–2 s, as the WebUI does. Revisit only if profiling shows a problem. |
| G2 full task fields | PDA-T-0156 | **REQUIRED** (tiny) | `TaskResponse` drops fields that `models.Task` already serialises. Add them as additive fields. |
| G3 structured conclusion | PDA-T-0157 | COVERED by G2 | `models.Task.Conclusion` reaches HTTP once G2 lands. The event half is not needed: polling sees the terminal status. |
| G4 pause/resume/retry | PDA-T-0158 | PARTIAL | Already exists as REST on the embedded mesnada server (`/api/tasks/:id/pause\|resume\|retry`). Mirror it into `/api/v1/orchestrator`. |
| G5 global session activity | PDA-T-0160 | PARTIAL | `GET /api/v1/instances/{id}/stream` (IPC bridge) plus `is_running`/`run_seq` polling work today. One generic `/api/v1/events` SSE would be more robust. |
| G6 decision proxy | PDA-T-0165 | NOT-NEEDED | The assistant owns its decision provider config. It reads Pando's config when there is no key or the key is an `$ENV` reference. |
| G7 chat attachments | PDA-T-0166 | NOT-NEEDED (later) | `agent.Run` already takes attachments; only `ChatRequest` lacks them. Not in the MVP scope. |
| G8 POST /sessions | PDA-T-0163 | NOT-NEEDED | Lazy creation through `chat/stream` with `sessionId:""` and `model`, then `PATCH` the title. Use the project child proxy for "project X". |
| G9 SSE ids / Last-Event-ID | PDA-T-0161 | NOT-NEEDED | `run`/`runSeq` + replay-on-reattach, which is the WebUI's own strategy. |
| G10 capabilities | PDA-T-0163 | COVERED | `GET /health` → `version`; `GET /api/v1/version`. Gate on version, or probe and handle 404. |
| G11 typed v2 DTO | PDA-T-0164 | NOT-NEEDED | A client-side mapper, the same one the WebUI uses. |
| G12 cross-session pending | PDA-T-0159 | PARTIAL → fold into G5 | Poll `GET /sessions` (`is_running`), then `GET /sessions/{id}/pending` for running sessions. Natively it belongs in the G5 stream. |
| G13 portable registry | PDA-T-0168 | NOT-NEEDED | Probe https then http on `web_port`. The hard-coded `/tmp/pando-instances` is the same path the client reads. |
| G14 message_complete | PDA-T-0162 | NOT-NEEDED | Accumulate `content_delta` until `done`; `steering_injected` splits bubbles. |
| G15 token mutex/persistence | PDA-T-0169 | NOT-NEEDED (for us) | The assistant never rotates the token; it refetches `/api/v1/token` on 401. The race is a Pando bug to report on its own. |
| G16 events/memory REST | PDA-T-0167 | NOT-NEEDED | `POST /api/v1/remembrances/kb/search` exists. Ask the Pando agent in chat (it has the remembrances tools) for "¿qué hicimos ayer?". |
| G17 WebUI/Wails bugs | PDA-T-0170 | NOT-NEEDED (for us) | Still present, but not on the assistant's path. Report them to Pando as plain bugs. |

**Recommended upstream changes:**

1. **PDA-T-0156 (G2)**: required, and it absorbs G3's task half.
2. **PDA-T-0160 (G5, widened to absorb G12)**: recommended, not blocking. The MVP uses polling.
3. **PDA-T-0158 (G4)**: optional, a cheap mirror of handlers that already exist. Without it,
   the assistant hides "pausa"/"reanuda".

Every other task (0155, 0157, 0159 merged into 0160, 0161, 0162, 0163, 0164, 0165, 0166,
0167, 0168, 0169, 0170) should be cancelled or parked as not needed.

## Why not switch to AG-UI?

AG-UI (`internal/agui`, mounted at `/api/v1/agui` when `[AGUI] Enabled`, or run as
`pando agui-serve`) gives typed events, `TEXT_MESSAGE_END` with `messageId`, typed
`Message[]` from `GET {path}/threads/{id}/messages`, reattach
(`GET {path}/threads/{id}/stream`), cancel (`POST {path}/runs/{id}/cancel`), fail-closed HITL
(`pando_permission_request` tool call, `hitl.go`) and per-thread `StateDoc.SubAgents`. The
following properties of AG-UI rule it out as the primary transport (`internal/agui/doc.go`,
invariants I2/I3/I7):

- It runs its **own** agent pool and its own permission and userinput services. It never
  sees WebUI or TUI sessions, their pending approvals, or runs resurrected there.
- It is off by default, has its own token (`~/.config/pando/agui-token` or `[AGUI]`), and
  has no orchestrator view and no project child proxy.
- It accepts multimodal parts but **drops** them (`input.go`, `InputContent`: "Only text is
  consumed today").

ADR 0002 stands: use the WebUI REST + SSE API. AG-UI stays an option for an isolated "assistant
profile" (`[AGUI.Profiles]`) if that is ever wanted. That option needs no Pando change.

## Per-gap analysis

### G1 · Task push events (PDA-T-0155): NOT-NEEDED

- **What exists:**
  - `GET /api/v1/orchestrator/tasks` (`internal/api/handlers_orchestrator.go:handleGetTasks`).
  - The WebUI polls it (`web-ui/src/components/orchestrator/OrchestratorView.tsx`
    `setInterval`; `ChatInfoSidebar.tsx` `SUBAGENT_POLL_MS`).
  - The orchestrator's durable event log (`internal/mesnada/events/events.go:Event`)
    records only terminal and guard kinds (`completed|failed|cancelled|gate_failed|…`), not
    created or progress events. A push stream would therefore need new plumbing anyway.
  - The AG-UI `StateDoc.SubAgents` (`internal/agui/subagents.go`) is per-thread and derived
    from tool traffic, so it is not global.
- **Assistant usage:** poll the list every 1–2 s while the window is visible and 5 s when
  idle. Diff by `id`+`status`+`progress` to produce cell birth and death. A local loopback
  call is cheap.
- **If ever needed:** fold it into the single generic `/api/v1/events` stream from G5 as a
  `task_changed {task}` event. Do not add a separate endpoint.

### G2 · `TaskResponse` with full fields (PDA-T-0156): REQUIRED (tiny, generic)

- **What exists:**
  - `models.Task` (`pkg/mesnada/models/task.go`) already has JSON tags for `work_dir`, `tags`,
    `error`, `exit_code`, `output_tail`, `started_at`, `completed_at`, `dependencies`,
    `acp_session_id`, `acp_mode`, `acp_status`, `parent_session_id`, `project_id`,
    `project_path`, `project_name` and `conclusion`.
  - `taskToResponse` (`handlers_orchestrator.go`) projects them away, and `tokens` is
    always 0.
  - The full model is only reachable through the embedded mesnada server
    (`internal/mesnada/server/gin.go`: pause/resume/retry return `{"task": task}`). That
    server is a separate port (default 9767, `internal/config/config.go`), is shifted when
    busy, has no token when embedded (`internal/app/app.go`, `mesnadaServer.New` without
    `Token`), is not in the instance registry, and is skipped in `mcp-server` mode. It is
    not a surface to build on.
- **Why indispensable:** the assistant cannot map a task to its project or parent session
  (`project_*`, `parent_session_id`), detect the ACP "waiting for permission" state
  (`acp_status`), or speak a conclusion (`conclusion.summary`/`follow_up`) without it.
- **Minimal generic design:** add the missing fields to `TaskResponse` as **additive,
  `omitempty`** fields copied from `models.Task`, and drop or fix `tokens`. Old clients are
  unaffected. The WebUI benefits too (it could show errors and conclusions). Add a unit
  test on `taskToResponse`. Do not add `?full=1` or a new DTO.

### G3 · Structured conclusion (PDA-T-0157): COVERED by G2

- **Task half:** once G2 lands, `GET /api/v1/orchestrator/tasks/{id}` carries `conclusion`
  (`status: success|partial|failed|blocked`, `summary`, `follow_up`, `confidence`).
- **Event half:** `conclusion_queued`/`conclusion_injected`/`resurrected`
  (`handlers_chat.go:dispatchSSEEvent`) carry only a human note. The assistant does not need
  a task id there: the task poll (G1) sees the terminal status and reads the conclusion.
  Treat these chat events as UI hints only.
- **Action:** cancel PDA-T-0157, or merge it into PDA-T-0156.

### G4 · Pause/resume/retry over HTTP (PDA-T-0158): PARTIAL

- **What exists:**
  - `Orchestrator.Pause`, `Resume(ctx, id, ResumeOptions)` and `Retry(ctx, id,
    RetryOptions)`.
  - These are already exposed as REST on the embedded mesnada server
    (`internal/mesnada/server/gin.go:handleAPITaskPause|handleAPITaskResume|handleAPITaskRetry`,
    routes `POST /api/tasks/:id/pause|resume|retry`) and as MCP tools
    `pause_task`/`resume_task`.
  - The main API has only `cancel` (`routes.go`, `POST /api/v1/orchestrator/tasks/{id}/cancel`).
- **Degraded path:** the assistant offers "cancela" only and hides "pausa"/"reanuda". As a
  workaround the user can ask the Pando agent in chat ("pausa la tarea X"), since the coder
  agent has the mesnada tools.
- **Minimal generic addition (optional):** mirror the three handlers into
  `POST /api/v1/orchestrator/tasks/{id}/{pause,resume,retry}` with the same bodies
  (`resume {prompt, model?, tags?}`, `retry {force?}`), returning `TaskResponse`. This is
  about 60 lines of glue with no new orchestrator logic. The WebUI orchestrator view could
  use it too.

### G5 · Global session activity stream (PDA-T-0160): PARTIAL

- **What exists:**
  - **`GET /api/v1/instances/{id}/stream`** (`handlers_instances.go:handleInstanceStream`)
    proxies an instance's ZMQ PUB as SSE. On the IPC leader, `internal/ipc/bridge/bridge.go`
    publishes, for **all** sessions of `CoderAgent`, the topics `session.update`,
    `session.deleted`, `llm.token`, `tool.start`, `tool.end`, `llm.end` and
    `instance.heartbeat` (`internal/ipc/protocol/topics.go`). Runs resumed by the delegation
    supervisor go through the same `CoderAgent` (`resume_run.go:takeResumedRun`), so they
    show up there too.
  - Limits of that stream: it works only when the server process is the IPC **primary**
    (`cmd/ipc_wiring.go:wireIPCRole`; a follower publishes nothing). It has no
    permission or question topics, no explicit run start, and it is unavailable in
    project-child mode.
  - Polling fallback: `GET /api/v1/sessions` returns `is_running` per session
    (`handlers_sessions.go`), and `GET /api/v1/sessions/{id}/pending` returns `running` and
    `run_seq` (`handlers_questions.go:handleSessionPending`).
- **Assistant usage today:** use the instance stream for the instance the client
  discovered (`instance_id` from the registry) when it is primary. Otherwise poll
  `/sessions?limit=20` every 2–4 s and react to `is_running` flips and `run_seq` changes.
- **Minimal generic addition (recommended, not blocking):** one `GET /api/v1/events` SSE
  that multiplexes pubsub Pando already has: `Sessions.Subscribe` (created, updated,
  deleted), `bgRunner` and `CoderAgent` busy transitions (`run_started` / `run_finished`
  with `sessionId`, `runSeq`), and `Permissions.Subscribe` / `UserInput.Subscribe`
  (`pending_changed` with the session id, see G12). It needs no new agent event types, and
  any client (WebUI included) could stop polling `/pending`.

### G6 · Decision proxy (PDA-T-0165): NOT-NEEDED

- **What exists:** `GET /api/v1/config/decision-model` (key masked).
  `POST /api/v1/decision-model/router/test` is only a health ping.
  `POST /api/v1/model-auto-mode/playground` (`handlers_model_auto_mode.go`) runs only the
  model router, not arbitrary questions.
- **Why not needed:** the `decide` crate already abstracts the transport. The assistant
  keeps its own provider settings (local Ollama by default, or a key in the OS keychain).
  It can prefill them from Pando's config when the key is absent or an `$ENV` reference.
  Adding a generic "run arbitrary prompt on Pando's decision model" endpoint would widen
  Pando's attack surface for one client.

### G7 · Attachments in `chat/stream` (PDA-T-0166): NOT-NEEDED (later)

- **What exists:** `agent.Service.Run(ctx, sessionID, content, attachments ...message.Attachment)`
  (`internal/llm/agent/agent.go`) already takes attachments. `ChatRequest`
  (`handlers_chat.go`) has only `sessionId`, `prompt` and `model`.
- **If wanted later:** add `attachments: [{name, mime, data}]` to `ChatRequest` and pass it
  through. That is generic and small, but it is a feature, not an MVP blocker. Screenshots
  can be referenced by path in the prompt meanwhile.

### G8 · `POST /api/v1/sessions` (PDA-T-0163, part 1): NOT-NEEDED

- **What exists:** `chat/stream` with an empty `sessionId` creates the session and accepts
  `model` (`handleChatStream` → `getOrCreateSession`, `applyRequestModel`). `PATCH
  /api/v1/sessions/{id}` renames it. For "nueva conversación en el proyecto X", use the child
  proxy `/api/v1/projects/{id}/web/api/v1/chat/stream`.
- **Degraded behaviour:** the session appears with the first prompt, which is how voice
  works anyway.

### G9 · SSE event ids / `Last-Event-ID` (PDA-T-0161): NOT-NEEDED

- **What exists:** every stream starts with `run {sessionId, runSeq}`
  (`streamSessionEvents`), and `GET /sessions/{id}/stream` replays the bgRunner buffer.
  The client resets the bubble on replay. AG-UI does not use event ids either.

### G10 · Capabilities (PDA-T-0163, part 2): COVERED

- **What exists:** `GET /health` (unauthenticated) returns `version` and `startup_mode`, and
  `GET /api/v1/version` (`handlers_base.go:handleVersion`) also returns it. The client
  gates on a minimum version, or feature-probes and treats 404/405 as "absent", which is the
  same degradation the epic already requires.

### G11 · Typed v2 DTO (PDA-T-0164): NOT-NEEDED

- PascalCase and untagged parts are stable, and the WebUI parses them the same way. A serde
  mapper in `pando-client` with recorded fixtures is enough. AG-UI's
  `threads/{id}/messages` returns typed messages, but only for AG-UI threads.

### G12 · Cross-session pending (PDA-T-0159): PARTIAL, fold into G5

- **What exists:** `GET /api/v1/sessions/{id}/pending` (`handleSessionPending`) is
  per-session. `Permissions.PendingRequests(sessionID)` and `UserInput.PendingRequests`
  are keyed by session.
- **Degraded usage:** for each session with `is_running: true`, poll its `/pending` every
  ~4 s. That is the WebUI's own cadence, and a blocked agent is always a running session.
  ACP-task permissions are visible in `acp_status` once G2 lands.
- **Native fix:** emit `pending_changed` on the G5 stream. There is no need for a separate
  `GET /api/v1/pending`.

### G13 · Portable instance registry (PDA-T-0168): NOT-NEEDED

- The registry path `/tmp/pando-instances` is hard-coded on both sides, so the client reads
  the same path Pando writes on every OS. The scheme comes from probing
  `GET {https|http}://localhost:{web_port}/health`, with `mode` as a hint (`desktop` = http).

### G14 · `message_complete` event (PDA-T-0162): NOT-NEEDED

- `done` ends the run. The final text is the accumulated `content_delta` since the last
  `steering_injected` or `tool_call`. `AgentEventTypeResponse` is swallowed on purpose
  (`dispatchSSEEvent`). That is enough to know when to speak.

### G15 · Token mutex and persistence (PDA-T-0169): NOT-NEEDED for the assistant

- The assistant never calls `regenerate-token`. It fetches `/api/v1/token` on start and on
  401. The unguarded write in `handlers_extras.go:handleRegenerateAPIToken` is a real data
  race in Pando: report it as a Pando bug, not as an assistant dependency.

### G16 · Events and memory REST (PDA-T-0167): NOT-NEEDED

- **What exists:** `POST /api/v1/remembrances/kb/search` and `/remembrances/code/search`
  (`handlers_remembrances_search.go`). Memories are KB-backed. For "¿qué hicimos ayer?", the
  assistant sends the question to the Pando agent via `chat/stream`. The agent has the
  `search_events`/`recall` tools and answers in natural language, which is what voice
  needs.

### G17 · WebUI/Wails bugs (PDA-T-0170): NOT-NEEDED for the assistant

- These bugs are still present: `web-ui/src/components/orchestrator/CreateTaskDialog.tsx`
  sends `description`, and `handleCreateTask` requires `prompt`.
  `internal/desktop/notifications.go:listenSSE` sends no token. None of them is on the
  assistant's code path. File them as Pando bugs (upstream issue or PR with owner approval),
  outside this epic.

## Recommended backlog changes (for the director)

| Task | Recommendation |
|---|---|
| PDA-T-0156 (G2) | Keep, and absorb G3's task half. The only **required** upstream change. |
| PDA-T-0160 (G5) | Keep with lower priority. Widen it to one generic `/api/v1/events` (sessions, runs, pending), absorbing G12. The assistant ships with polling first. |
| PDA-T-0158 (G4) | Optional: mirror the existing mesnada REST handlers. Otherwise hide pause/resume. |
| PDA-T-0155, 0157, 0159 | Cancel (covered by polling, G2, or G5). |
| PDA-T-0161, 0162, 0163, 0164, 0165, 0166, 0167, 0168 | Cancel or park as not needed. Document the client-side degradations above in the `pando-client` tasks. |
| PDA-T-0169, 0170 | Not assistant work. Report to Pando as bugs. |
