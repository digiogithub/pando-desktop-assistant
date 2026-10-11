# Pando HTTP API: research for a native Rust/GPUI client (chat + voice assistant)

Source: `/www/MCP/Pando/pando` at commit `839540537`, read on 2026-10-11. All paths below are relative to that repo unless stated otherwise.
Planning docs reviewed: `pando-assistant/docs/01-strategy.md`, `03-decision-layer.md`, `04-pando-api.md`, `06-ui-mapping.md`.

---

## 0. TL;DR: corrections to the planning docs

| Doc claim | Reality | Evidence |
|---|---|---|
| Auth is a "bearer token" | Send the token as the **`X-Pando-Token: <token>` header** or a **`?token=`** query parameter. `Authorization: Bearer` is only accepted on the AG-UI routes; elsewhere it gets a 401. | `internal/api/server.go:630-639`, `:699-705` |
| The token is persisted somewhere (`~/.pando/...`) [verify] | **It is not persisted.** It is 32 random bytes in hex, generated at each process start (`server.go:286-292`, `:598-603`). Obtain it with **`GET` or `POST /api/v1/token`**, which returns `{"token":"…"}` without credentials when the server is bound to loopback (`server.go:644-693`, `handlers_base.go:59-63`). That is exactly how the web UI and the Wails window get it. Only the MCP and AG-UI listener tokens are persisted (`~/.config/pando/{mcp,agui}-token`, `cmd/listener_token.go:14-41`), and they are not the API token. | see left |
| `POST /api/v1/config/api-server/regenerate-token` rotates the token | It does, but only in memory (16 bytes, a different length), with no lock and no persistence. Every other client (the web UI tab, the desktop notifier) is then logged out. The assistant should not call it. | `handlers_extras.go:52-67` |
| `GET/POST /api/v1/sessions` creates a session | **No POST exists.** `/api/v1/sessions` is GET only and paginated. A session is created implicitly by `POST /api/v1/chat/stream` with an empty `sessionId`; the new id arrives in the first `session` SSE event. | `handlers_sessions.go:40-44`, `handlers_chat.go:877-896` |
| SSE event `todos_updated` | The name is **`todos_update`**. The TodoWrite tool is also turned into **`plan_update`**, and that is what the web UI renders. | `handlers_chat.go:641-647`, `:721-758` |
| SSE event `response` (speak it) | **There is no `response` event on the wire.** It is swallowed (`handlers_chat.go:711-712`). Build the final text by accumulating `content_delta` until `done`. | |
| Event list omitted some events | There are also `run`, `tool_call_update`, `plan_update`, `goal_status`, `permission_request`, `question_request` and `done`. | §3 |
| `conclusion_*` / `resurrected` carry the conclusion | They carry only a **human note** such as `"📥 Delegated result queued — …"` and `"🔁 Resuming — a delegated task reported its result."`. They have no task id, summary or status. | `internal/llm/agent/agent.go:706-708`, `:752-756` |
| Notifications stream reports task created/finished and permission requests | **No.** It only carries user-facing toasts: LLM retries, tool errors, LSP messages (`{id,time,level,source,message,ttl}`). There is no task lifecycle event and no permission event in it. | `handlers_notifications.go`, `internal/notify/notify.go:13-41` |
| Task JSON = full `models.Task` (work_dir, acp_status, conclusion, …) | The HTTP API returns a **projection, `TaskResponse`**: `id, name, prompt, agent(=engine), model, persona, status, progress(int %), tokens(always 0), output, current_tool, tool_calls, created_at, updated_at`. It omits **work_dir, tags, error, exit_code, output_tail, acp_session_id, acp_status, parent_session_id, project_*, conclusion, dependencies**. | `handlers_orchestrator.go:11-60` |
| No pause endpoint [verify] | **Confirmed: there is no HTTP pause or resume.** `Orchestrator.Pause` and `Resume` exist (`internal/mesnada/orchestrator/orchestrator.go:1403-1520`) and are exposed only as mesnada MCP tools `pause_task` / `resume_task` (`internal/mesnada/server/tools.go:58-59,415-445`). Resume is not a resume: it **spawns a new task** and requires a `prompt`. | |
| Conclusion `status: done\|partial\|blocked\|failed` | The vocabulary is **`success\|partial\|failed\|blocked`**. `done` normalises to "" and the enricher then picks a value from the terminal state. | `internal/mesnada/conclusion/parse.go:147-160`, `pkg/mesnada/models/task.go:200-215` |
| `POST /api/v1/decision-model/router/test` "lets the assistant test its question set" | It is only a **health probe**. It sends a fixed "ping" question (`systemone/provider.go:134-148`). **No Pando endpoint proxies arbitrary `/v1/systemone` questions.** | `handlers_decision_model.go:304-332` |
| The assistant can call the decision provider using Pando's config | It can read `provider`, `effectiveBaseURL`, `model` and `keepAlive`, but the **API key is masked** (`apiKeyMasked`). That works for local Ollama (no key) and for TypeSafe when the key is an `$ENV` reference (read `TYPESAFE_API_KEY` yourself); otherwise it cannot authenticate. | `handlers_decision_model.go:53-87` |
| Served by `pando serve` | `pando serve` and `pando app` default to **HTTPS** (auto-generated local CA). `pando desktop` uses **plain HTTP**. A plain `pando` (TUI) or ACP instance runs **no HTTP API**. Default port is 8765 with fallback to 8766..8775, then a random port. | §1.2 |

Other surprises:
- Session and Message JSON use **PascalCase keys**, because the Go structs have no json tags: `ID`, `Title`, `Parts`, … (`internal/session/session.go:135-149`, `internal/message/content.go:145-153`).
- Message parts are **untagged objects**. You discriminate them by which keys are present (§3.6).

---

## 1. Discovery and auth

### 1.1 Which processes serve the API

| Command | HTTP API | Scheme | Port | Registry `mode` | Source |
|---|---|---|---|---|---|
| `pando serve` | yes (API only, no UI) | **https** (auto TLS) | `--port` default 8765; fallback +1..+10, then random | `webui` | `cmd/serve.go:56-91,132-197,274-280` |
| `pando app` | yes, plus the embedded web UI | **https** | same as serve | `webui` | `cmd/app.go:26-56,106-162`, `cmd/app_command.go:16-19` |
| `pando desktop` | yes, plus the web UI, plus a spawned Wails window | **http** | same as serve | `desktop` | `cmd/desktop.go:85-186` |
| `pando` (TUI), `pando acp`, proxy | **no HTTP API** | n/a | n/a | `tui` / `acp` / `proxy` | `cmd/root.go:224-231,594-601` |
| Project child (`PANDO_CHILD_API_TOKEN` env) | yes, but its token is held only by the parent; reach it through the parent proxy | https | fixed | `webui` with `parent_instance_id` | `cmd/startup_mode.go:57`, `cmd/serve.go:63-80` |

TLS: `tlsutil.EnsureCert(config.TLSCertDir(...))` writes `ca.crt`, `ca.key`, `server.crt` and `server.key` under **`$XDG_CONFIG_HOME/pando/tls/`** (default `~/.config/pando/tls/`) (`internal/tlsutil/cert.go:29-32,83-93`; `internal/config/global_projects.go:39-59`). A native client should **add `~/.config/pando/tls/ca.crt` as a trusted root** in reqwest/rustls rather than disabling verification.

### 1.2 Instances registry (server discovery)

- Location: **`/tmp/pando-instances/<instance_id>.json`**, one file per live process. The path is hard-coded. Stale files are pruned on read by checking whether the PID is alive (`internal/instanceregistry/registry.go:14-63`).
- Entry (`entry.go:25-46`):
  ```json
  {"instance_id":"uuid","path":"/abs/cwd","pid":123,"pub_port":0,"rpc_port":0,
   "web_port":8765,"started_at":"RFC3339","mode":"tui|webui|desktop|acp|noninteractive|proxy",
   "parent_instance_id":"","is_primary":true}
  ```
- The entry has **no scheme and no token**. Discovery algorithm for the client:
  1. List `*.json`, keep entries with `web_port > 0`, and prefer `mode == desktop` (http) or `webui` (https).
  2. Probe `GET {scheme}://localhost:{web_port}/health`. Try `https` first for `webui`, and `http` for `desktop`. `/health` is unauthenticated and returns `{status, version, startup_mode, parent_instance_id, project_id, project_name, public_base_path, pid?}` (`handlers_base.go:12-57`).
  3. `GET /api/v1/token` returns `{token}`. This works on loopback only. Otherwise it needs Basic auth (`[Server.BasicAuth]`, `Authorization: Basic …`), or a 401 comes back with a `reason`.
  4. Fallback: `https://localhost:8765`, then `http://localhost:8765`.
  - Also available once authenticated: `GET /api/v1/instances` (same data plus per-instance proxies).
- On Windows and macOS `/tmp` differs. The path is literally `/tmp/pando-instances`, so on Windows it resolves relative to the current drive. That is a portability gap (§6).

### 1.3 Auth details

- Middleware chain: `cors(basicAuth(auth(mux)))` (`server.go:338-341`).
- **Public paths:** `/health`, non-`/api` static paths, and `/api/v1/token` (under the conditions above).
- **Every `/api/*` request needs** `X-Pando-Token`, or `?token=` for EventSource and WebSocket (`server.go:630-639,665-724`).
- Send **`X-Pando-Client: <name>`**. When basic auth is enforced it suppresses the `WWW-Authenticate` challenge (`basicauth.go:16-19,102-105`). The web UI sends `web`.
- Basic auth applies only on non-loopback binds that have users configured (`basicauth.go:46-64`).
- **CORS:** `Access-Control-Allow-Origin: *`; allowed headers `Content-Type, X-Pando-Token, X-Pando-Client, Authorization` (`server.go:606-628`). This does not matter for a native client.
- **No rate limiting** anywhere in `internal/api` (grep found none). Request bodies have a 30 s `ReadTimeout`. `WriteTimeout` is 0, so streams are unbounded (`server.go:343-348`).
- How the existing clients get the token:
  - **Web UI:** `authenticate()` uses the token stored in localStorage `pando_token` (suffixed `@<fnv(basePath)>` when a base path is set). Otherwise it calls `POST /api/v1/token {}`. On 401 with `basic_auth_required` it shows a login dialog and retries with Basic auth (`web-ui/packages/pando-client/src/services/auth.ts:8-39`, `api.ts:4,107-171`).
  - **Wails desktop:** the same web UI inside a webview pointed at `pando desktop`'s own server. **No token is injected.** The `GetServerInfo` binding mentioned in `desktop/README.md:27-31` and `web-ui/src/services/desktop.ts:18-31` no longer exists (stale).
  - **SDKs (TS, Python, Java, .NET HTTP clients)** send `Authorization: Bearer`, which **gets 401** on `/api/v1/*`. They also call endpoints that do not exist (`POST /api/v1/sessions`, `/sessions/{id}/messages`). **Do not use them as a reference.**

### 1.4 Multi-project

- `GET /api/v1/projects` returns `{projects:[projectResponse]}`. `projectResponse` is `{id,name,path,status: running|stopped|error|initializing|missing, initialized, external, delegations, delegation_spawned, acp_pid, web_state, web_port, web_url, last_opened, created_at, updated_at}` (`handlers_projects.go:15-34`).
- `POST /api/v1/projects/{id}/web/open` starts a **project child** server. Its API is then reachable **through the parent** at `/api/v1/projects/{id}/web/api/v1/...`. The proxy strips the caller's token and injects the child's token, and SSE flushes immediately (`FlushInterval:-1`) (`handlers_projects_web.go:125-210`). One authenticated connection to the primary can therefore drive every project.
- `POST /api/v1/projects/{id}/open-desktop` exists only in `desktop` mode. It spawns `pando desktop --cwd` (`handlers_projects_desktop.go:43-90`).

---

## 2. Full endpoint inventory (`internal/api/routes.go:16-318`)

Legend for **Need**:
- **C** = core for a native chat + assistant client
- **A** = assistant/voice specific
- **O** = optional or later
- **–** = not needed (admin, settings UI, design studio)

### 2.1 Server, auth, version

| Method | Path | Need | Notes |
|---|---|---|---|
| GET | `/health` | C | unauthenticated; liveness + startup mode |
| GET/POST | `/api/v1/token` | C | `{token}` |
| GET | `/api/v1/version` | C | `{version, latest?, update_available, update_command?, release_url?, checkable}` (`internal/updatecheck/status.go:22-36`). There is **no API/protocol version**, see §6 |
| GET | `/api/v1/project`, `/api/v1/project/context` | C | `{cwd, version}` |
| POST | `/api/v1/config/api-server/regenerate-token` | – | do not call |
| * | `/api/v1/config/api-server/{external-access,basic-auth,…}` | – | |

### 2.2 Sessions and chat

| Method | Path | Need | Request → Response |
|---|---|---|---|
| GET | `/api/v1/sessions?limit=&offset=` | C | → `{sessions:[Session+{is_running,run_seq,prompt_preview?}], total, limit, offset, has_more}`. Newest first (`handlers_sessions.go:40-97`) |
| GET | `/api/v1/sessions/{id}` | C | → `{session, messages:[Message], is_running, run_seq}` (`:118-141`) |
| PATCH | `/api/v1/sessions/{id}` | C | `{title}` → `{session}` |
| DELETE | `/api/v1/sessions/{id}` | C | → `{status:"deleted"}` (the web UI does not expose this) |
| POST | `/api/v1/chat/stream` | **C/A** | `{sessionId?, prompt, model?}` → SSE (§3). GET with `?sessionId=&prompt=` also works |
| POST | `/api/v1/chat` | O | same body, blocking → `{sessionId, messageId(empty), response}` |
| GET | `/api/v1/sessions/{id}/stream` | **C/A** | reattach: replay + live (§3.4) |
| POST | `/api/v1/sessions/{id}/steer` | **C/A** | `{prompt}` → 202 `{sessionId, queued}`; 409 `{error:"not_busy",hint}` |
| POST | `/api/v1/sessions/{id}/cancel` | **C/A** | → `{sessionId, cancelled:true}` (`resume_run.go:127-139`) |
| GET | `/api/v1/sessions/{id}/pending` | **C/A** | → `{permissions:[…], questions:[…], running, run_seq}`. This doubles as the run heartbeat (`handlers_questions.go:70-111`) |
| POST | `/api/v1/permissions/respond` | **C/A** | `{id, sessionId, action:"allow"\|"allow_session"\|"deny"}` → `{status:"ok"}` |
| POST | `/api/v1/questions/respond` | **C/A** | `{id, sessionId, answers:[{questionId, selected:[label…], otherText}], cancelled?}` |
| GET/POST | `/api/v1/sessions/{id}/auto-approve` | C | POST `{enabled}` → `{sessionId, enabled}` |
| GET | `/api/v1/sessions/{id}/model` | C | → `{sessionId, model, override, autoSelected}` |
| GET | `/api/v1/sessions/{id}/goal` | C | → `{goal: GoalPayload\|null}` |
| POST | `/api/v1/sessions/{id}/goal/cancel` | C | → `{goal}`; 404 "no active goal" |
| GET | `/api/v1/commands` | C | → `[{name, description, acceptsArgs}]`. Built-in + custom + extension commands (`internal/commands/registry.go:12-47`). They are sent as prompt text and intercepted server-side (`handlers_chat.go:191-195,927-1201`) |

### 2.3 Models, personas, auto mode

| Method | Path | Need | Notes |
|---|---|---|---|
| GET | `/api/v1/models` | C | → `{models:[ModelInfo], errors, autoSelected}`. `ModelInfo` = `{id,name,provider,accountId?,description,badges[],canReason,supportsReasoningEffort,supportsAttachments,contextWindow?,maxOutputTokens?,costPer1MIn?,costPer1MOut?,knowledge?,releaseDate?, routerProvider?,routerModel?,routerHealthy?,routerProblems?}`. A synthetic `auto` entry comes first when auto mode is on (`handlers_models.go:22-43,337-345`) |
| PUT | `/api/v1/models/active` | C | `{model, sessionId?}` → `{model, scope:"session"\|"config"}`. Without `sessionId` it **changes the global coder model in config** (`handlers_models.go:440-496`) |
| GET | `/api/v1/personas` | O | `{personas:[string]}` |
| GET/PUT | `/api/v1/personas/active[?sessionId=]` | O | PUT `{name}`; `""` = Auto |
| GET/PUT | `/api/v1/config/model-auto-mode` | O | |
| POST | `/api/v1/model-auto-mode/playground` | – | dry-run of model routing only |

### 2.4 Orchestrator (mesnada)

| Method | Path | Need | Notes |
|---|---|---|---|
| GET | `/api/v1/orchestrator/tasks` | **A** | → `{tasks:[TaskResponse], total}`. No filters, no pagination, **no SSE** |
| POST | `/api/v1/orchestrator/tasks` | A | `{prompt, work_dir?, model?, engine?, tags?, dependencies?, background}` (background is forced to true) → 201 `TaskResponse`. Note: the web UI's `CreateTaskDialog` sends `{name, description, model}`, which gets a 400 "prompt is required". **That is a web UI bug** |
| GET | `/api/v1/orchestrator/tasks/{id}` | A | `TaskResponse` (projection, see §0) |
| POST | `/api/v1/orchestrator/tasks/{id}/cancel` | A | 409 if already terminal |
| DELETE | `/api/v1/orchestrator/tasks/{id}` | A | cancel + remove, 204 |
| GET | `/api/v1/orchestrator/delegation/metrics` | O | `{warm_attempts, warm_hits, external_hits, warm_failures, cold_fallbacks, cap_rejections, resurrections, live_injections, external_reattach_recovered, external_reattach_failed, warm_hit_rate}` |
| GET/POST/PUT/DELETE | `/api/v1/cronjobs[/{name}[/run]]` | O | |

`TaskResponse` (`handlers_orchestrator.go:11-26`):
```json
{"id":"…","name":"first 80 chars of prompt","prompt":"…","agent":"claude|pando|acp-claude|…","model":"…",
 "persona":"…","status":"pending|running|paused|completed|failed|cancelled","progress":0,"tokens":0,
 "output":"…","current_tool":"…","tool_calls":[{"id","name","title","kind","arguments","result","status","locations":[],"diffs":{},"started_at"}],
 "created_at":"RFC3339","updated_at":"RFC3339"}
```
`tool_calls` has the shape of `pkg/mesnada/models/task.go:217-228`.

### 2.5 Projects, instances, IPC

| Method | Path | Need | Notes |
|---|---|---|---|
| GET | `/api/v1/projects`, `/projects/active`, `/projects/{id}` | C/A | |
| POST | `/api/v1/projects` | O | `{path, name}` |
| POST | `/api/v1/projects/{id}/{activate,deactivate,stop,init}` | O | activate → 409 `{error:"project_needs_init"}`; stop → 409 `external_instance` |
| PATCH/DELETE | `/api/v1/projects/{id}` | O | |
| GET | `/api/v1/projects/events` (SSE) | C | named events `connected, switched, status_changed, init_required, delegation_changed, web_started, web_stopped, web_error`; data `{project_id,status,error,delegations,web_port}` (`handlers_projects.go:676-730`) |
| GET/POST | `/api/v1/projects/web`, `/projects/{id}/web/{open,close}` | C | start or stop a project child |
| * | `/api/v1/projects/{id}/web/…` | C | reverse proxy to the child (§1.4) |
| GET | `/api/v1/instances[/{id}]` | A | registry view (`handlers_instances.go:19-100`) |
| GET | `/api/v1/instances/{id}/stream` | O | SSE of raw **ZMQ PUB envelopes** (unnamed `data:` events) `{instanceId, projectId, sessionId?, topic, timestamp, payload}` (`internal/ipc/envelope.go:13-26`). Topics include `session.update`, `session.activated`, `session.deleted`, `message.append`, `chat.message`, `system.ready`, `instance.heartbeat\|shutdown\|promoted`. This is the only path to observe **TUI** instances |
| GET | `/api/v1/instances/{id}/sessions[/{sid}[/messages\|/stream]]` | O | RPC to other instances. Messages are `{id, role, content, created_at}` |
| POST | `/api/v1/instances/{id}/sessions/{sid}/message` | O | `{content}`. This sends to a TUI session |
| DELETE | `/api/v1/instances/{id}/sessions/{sid}/cancel` | O | note it is DELETE, not POST |
| GET | `/api/v1/ipc/status` | O | |

### 2.6 Events, logs, notifications

| Method | Path | Need | Payload |
|---|---|---|---|
| GET | `/api/v1/notifications/stream` | C | `event: connected` `{}` once; then `event: notification` `{id, time(RFC3339), level: info\|warn\|error, source: llm_provider\|tool\|lsp\|agent\|system, message, ttl(ns; 0 = sticky)}` |
| GET | `/api/v1/config/events` | O | `event: connected {ts}`; then **unnamed** `data: {section, timestamp, source: tui\|webui\|file\|overlay, event?}` (`internal/config/eventbus.go:9-20`) |
| GET | `/api/v1/logs?limit=` | O | `{logs:[{id,timestamp,level,source,message,details?}], total}` |
| GET | `/api/v1/logs/stream` | O | `event: log` (`handlers_logs.go:153,177`). The web UI polls instead |
| GET | `/api/v1/container/events` | – | |

### 2.7 Files, VCS, terminal

| Method | Path | Need | Notes |
|---|---|---|---|
| GET | `/api/v1/files?path=` | O | directory listing |
| GET/PUT/DELETE | `/api/v1/files/{path}` | O | `{path, content}` |
| POST | `/api/v1/files`, `/files/rename` | O | |
| GET | `/api/v1/files/search` | C (for @-mentions) | the web UI does not use it |
| GET | `/api/v1/files/raw/{path}?token=` | O | binaries and images |
| GET | `/api/v1/fs/browse?path=` | O | directory picker |
| * | `/api/v1/snapshots…`, `/api/v1/agentvcs/…` | C (diff views) | `agentvcs/sessions/{id}/diff` → `{diff:[{path,type,old_hash?,new_hash?}]}`; `agentvcs/blobs/{hash}` → text; revert / revert-files |
| GET (WS) | `/api/v1/terminal/pty?cols=&rows=&session_id=&token=` | O | binary I/O + JSON control frames |
| POST | `/api/v1/terminal/exec` | O | `{command, session_id}` |

### 2.8 Memory, KB, search (REST, no MCP needed)

| Method | Path | Need | Body |
|---|---|---|---|
| POST | `/api/v1/remembrances/kb/search` | O | `{query, limit, tags, sort_by_date, exclude_outdated?, scope, path_prefix}` (`handlers_remembrances_search.go:23-31`) |
| POST | `/api/v1/remembrances/code/search` | O | `{project_id, query, limit, offset, languages, symbol_types, min_score, include_docs}` (`:184-192`) |
| POST/DELETE | `/api/v1/remembrances/kb/documents`, `kb/reindex` | – | |
| GET/PUT | `/api/v1/remembrances/enrichment` | O | toggles context enrichment |
| * | `/api/v1/remembrances/{projects,reindex,…}` | – | |

There is **no REST access to events or memory (`remember`/`recall`)**. Those exist only as MCP tools.

### 2.9 Decision model

| Method | Path | Need | Notes |
|---|---|---|---|
| GET | `/api/v1/config/decision-model` | **A** | → `{router:{provider: ollama\|typesafe\|custom, baseURL, effectiveBaseURL, model, keepAlive?, headers?, apiKeySet, apiKeyMasked}, timeoutMs, warnings[]}` (`handlers_decision_model.go:24-87`, `handlers_model_auto_mode.go:24-33`). `timeoutMs` 0 means 1500 for ollama and 3000 for remote (`internal/config/decision_model.go:33-44`) |
| PUT | `/api/v1/config/decision-model` | – | full `DecisionModelConfig` `{router:{provider,baseURL,apiKey,model,keepAlive,headers}, timeoutMs, clearApiKey?}` |
| DELETE | `/api/v1/config/decision-model/api-key` | – | |
| GET | `/api/v1/decision-model/router/health` | **A** | → `{ok, report:HealthReport, problems}`, cached 60 s. `HealthReport` = `{ok,kind,model,version?,reachable,authorized,versionOK,modelPresent,isDecisionModel,remote,latencyMs,problems[]}` (`internal/llm/systemone/provider.go:82-95`) |
| POST | `/api/v1/decision-model/router/test` | A | body optional `{router?:draft}` → same as health, uncached |
| GET/POST | `/api/v1/decision-model/router/models` | O | `{models:[{id,name,contextWindow?,capabilities?}], status: filtered\|unfiltered\|unsupported, suggestions?, hint?, error?}` |
| POST/GET | `/api/v1/decision-model/router/pull[/{id}]` | O | Ollama pull job |
| * | `/api/v1/model-auto-mode/router/*` | – | deprecated aliases |

### 2.10 Everything else (settings UI): need "–"

These cover `settings`, `settings/providers`, all `config/*` sections (providers, agents, mcp-servers, mcp-gateway, lsp, tools, browsers, openlit, sandbox, bash, token-optimization, extensions, locked-keys, ui-policy, services, evaluator, provider-accounts, provider-types, init-status, generate), `ui/preferences`, `extensions/{ui,memory,license}`, `mcp/{name}/{login,logout,status}`, `auth/providers/*`, `setup/*`, `skills/*`, `savings`, `container/*`, `evaluator/*`, `design/*` and the AG-UI routes.

A native client can defer all of these, or open the web UI for them (`/settings` on the same origin).

### 2.11 Voice-related endpoints

None exist. There is no STT, TTS or audio endpoint, and the web UI has no voice code either.

---

## 3. Chat streaming protocol (authoritative)

### 3.1 Request

`POST /api/v1/chat/stream`, `Content-Type: application/json`, `X-Pando-Token`. Body (`handlers_chat.go:35-39`):

```json
{"sessionId": "optional; empty creates a session titled 'New Chat'", "prompt": "required text", "model": "optional; 'auto' or a model id (session-scoped)"}
```

- Text only: **no attachments, images, files, mode or persona fields.**
- If `prompt` starts with `/`, it is parsed as a slash command and handled inline (`commands.Parse`).
- An unknown `model` is silently ignored. `auto` returns 400 when auto mode is disabled (`:62-81`).
- If the session is already busy, the prompt is **converted into steering** and the stream attaches to the running run (`:204-212`).
- The run executes in `BackgroundSessionManager`, so it **survives client disconnect** (`:197-203`).

### 3.2 Wire format

The format is `event: <name>\ndata: <single-line JSON>\n\n` (`handlers_chat.go:760-767`). Note what is missing:
- no `id:` lines
- no `retry:`
- no keep-alive comments: a silent stream during a long tool run is normal
- no `Last-Event-ID` support

Every response also sets `Access-Control-Allow-Origin: *`.

### 3.3 Event catalogue (order of a typical run)

| # | event | data | Notes |
|---|---|---|---|
| 1 | `session` | `{"sessionId":"…","running":true}` | the first event; carries the new id when one was created |
| 2 | `run` | `{"sessionId","runSeq":N}` | the run sequence for this session (`:340`) |
| – | `permission_request` | `{id, session_id, tool_name, description, action, path, params, justification?, grant_key?, require_explicit_approval?, never_auto_approve?}` | replayed for pending requests on attach. `action == "execute_unsandboxed"` means a sandbox escalation; `grant_key` is `prefix:<words>` or `exact:<cmd>` (`:410-427`, `internal/permission/permission.go:47-61`) |
| – | `question_request` | `{id, session_id, questions:[{id, header, question, multi_select, options:[{label, description}]}]}` | AskUserQuestion (`internal/userinput/userinput.go:18-37`) |
| – | `thinking_delta` | `{text}` | |
| – | `content_delta` | `{text}` | accumulate these for the final answer |
| – | `tool_call` | `{id, name, kind, title, status:"pending"\|"in_progress", input:"<raw JSON string>", locations:[{path}]}` | `kind` ∈ `execute, edit, read, search, fetch, think, switch_mode, other` (`internal/toolmeta/toolmeta.go:18-35`) |
| – | `tool_call_update` | `{id, status:"in_progress", kind, title, input, locations}` | when the input finished streaming |
| – | `tool_result` | `{tool_call_id, name, kind, title, status:"completed"\|"failed", content, is_error, locations, raw_output:{output, metadata?}, terminal?:{terminal_id, exit_code(0/1)}, diff?:{file_path, old_string?, new_string?} \| {file_path, new_content} (write), images?:["data:…"]}` | always preceded by a `tool_call` (`:525-639`). `raw_output.metadata` may carry `{additions, removals, files_changed}` |
| – | `plan_update` | `{entries:[{title, status, active_form?}]}` | from TodoWrite; its tool_call and result are suppressed (`:462-472,721-758`) |
| – | `todos_update` | `{session_id, todos:[{content, status: pending\|in_progress\|completed, priority: high\|medium\|low}]}` | agent `todos_updated` events (`internal/llm/tools/todo_write.go:13-17`). The web UI ignores it |
| – | `token_usage` | `{session_id, prompt_tokens, completion_tokens, context_window, estimated, cache_read_tokens, cache_creation_tokens, reasoning_tokens, cost}` | the breakdown is only meaningful when `estimated == false` |
| – | `system_message` | `{session_id, text, routing?:RoutingInfo, context_filter?:ContextFilterInfo}` | `RoutingInfo` (`internal/llm/agent/model_auto.go:37-65`): `{routeId?, model, matched, probability, confidence, reason, fallbackUsed, errorClass?, routerProvider?, routerModel?, routerLatencyMs, routerCostUsd?, candidates?, kind: routed\|no_match\|router_unavailable\|route_unusable\|failover, notice}`. `ContextFilterInfo` (`context_filter.go:43-57`): `{kept, dropped, bySource:{code\|kb\|events\|memory:{kept,dropped}}, threshold, latencyMs, routerProvider?, routerModel?, reason?, notice}` |
| – | `steering_queued` / `steering_injected` | `{session_id, message}` | the UI should close the assistant bubble, insert the steered user text, then open a new bubble |
| – | `conclusion_queued` / `conclusion_injected` | `{session_id, message}` | a human note only |
| – | `resurrected` | `{session_id, message}` | the first event of a system-initiated run (§3.5) |
| – | `goal_status` | `{goal: {objective, status, iteration, maxIterations, progress?, nextStep?, startedAt, completedAt?} \| null}` | emitted on attach and at the end; also by `/goal*` commands (`:47-60,816-823`) |
| – | `error` | `{error}` | |
| end | `done` | `{}` | the run finished. **If the body closes without `done`, the run may still be alive**: reattach |

Slash-command streams emit `session` → `content_delta`/`error`/`goal_status` → `session {running:false}`. They have **no `done`** event (`:1200`).

### 3.4 Reattach and reconnection

- `GET /api/v1/sessions/{id}/stream` emits, in order:
  1. `session {sessionId, running}`
  2. `goal_status`
  3. if nothing is running, `done` immediately
  4. otherwise a **replay of the background run's ring buffer**, then live events. The buffer is the last **500** agent events (oldest dropped) and is kept **10 min** after the run ends (`background_runner.go:10-20`). Pending permission and question requests are replayed too.
  - A run that the agent owns outside bgRunner (TUI, ACP) is followed live from the broker, **with no replay** (`resume_run.go:72-124`).
- No `Last-Event-ID` is supported. Recovery uses the **`run_seq`** protocol, which the web UI implements (`web-ui/packages/pando-client/src/services/runSeq.ts`, `ChatView.tsx:99-136`, `useChat.ts:575-628`):
  - On `GET /sessions/{id}`, set `seenRunSeq = is_running ? -1 : run_seq`.
  - Poll `GET /sessions/{id}/pending` every ~4 s. It returns `running` and `run_seq`.
  - If there is no open stream and `run_seq > seenRunSeq`, claim it and open `/sessions/{id}/stream`. The `run` event sets `seenRunSeq`.
  - If the replay produced no parts (because the buffer overflowed or expired), reload history with `GET /sessions/{id}`.
  - **Caveat:** a replay re-delivers everything since the run start, so the client must **reset the live bubble** before applying it. Deltas cannot be de-duplicated because there are no ids.
- Notifications, config, and project SSE: reconnect with exponential backoff (1 s doubling to 30 s), as the web UI does. They have no replay.

### 3.5 Delegation and resurrection flow (important for the assistant)

1. The coder agent delegates by calling the mesnada spawn tool. A `tool_call` / `tool_result` for that tool appears in the chat stream; the task id is inside `tool_result.content`.
2. When the subagent finishes:
   - **Parent run still active:** the conclusion is queued and then injected. SSE shows `conclusion_queued` then `conclusion_injected`, and the model reacts in the same stream.
   - **Parent idle:** the delegation supervisor starts a **new background run** owned by the API server (`resume_run.go:25-54`). **No client stream is attached.** The client only learns of it by seeing `run_seq` increase (via `/pending` or the session list `run_seq`/`is_running`), then reattaching. The first replayed event is `resurrected`.
3. The injected text is `FormatForParent` (`internal/mesnada/conclusion/format.go:26-104`), which produces:
   ```
   [delegated-result task=<id> engine=<e> project=<p> status=<success|partial|failed|blocked> confidence=0.90]
   summary: …
   follow_up: …
   artifacts: …
   ```
   It is injected into the conversation as a user/system message, so it appears in `GET /sessions/{id}` history and **not in SSE**.

   For voice, the assistant can either:
   - speak the parent's reply (`content_delta` of the resurrected run), or
   - parse the `[delegated-result …]` block from history.

   The structured `Conclusion` is **not reachable over HTTP** (gap G3).

### 3.6 History shape (`GET /api/v1/sessions/{id}`)

`session` (PascalCase, `internal/session/session.go:135-149`):
```json
{"ID","ParentSessionID","Title","MessageCount","PromptTokens","CompletionTokens","CacheReadTokens",
 "CacheCreationTokens","ReasoningTokens","SummaryMessageID","Cost","CreatedAt","UpdatedAt"}
```
The session list adds snake_case `is_running`, `run_seq` and `prompt_preview`. Timestamps are Unix seconds.

`messages[]` (`internal/message/content.go:145-153`):
```json
{"ID","Role":"user|assistant|system|tool","SessionID","Parts":[…],"Model","CreatedAt","UpdatedAt"}
```

`Parts` are untagged. Discriminate them by keys:

| Keys present | Part |
|---|---|
| `text` | TextContent |
| `thinking` | ReasoningContent |
| `id, name, input(JSON string), type, finished` (+`thought_signature`) | ToolCall |
| `tool_call_id, name, content, metadata(JSON string), is_error, images?` | ToolResult. Lives in separate `Role:"tool"` messages; join it to the ToolCall by id |
| `reason, time` | Finish: `end_turn \| max_tokens \| tool_use \| canceled \| error \| permission_denied \| unknown` |
| `url, detail?` | ImageURLContent |
| `Path, MIMEType, Data(base64)` | BinaryContent (user attachments from other surfaces) |

The web UI mapper is `web-ui/packages/pando-client/src/services/mappers.ts`. It folds tool results into tool calls and drops `tool` messages.

---

## 4. How the existing clients consume the API

### 4.1 Web UI: `web-ui/` (React 19 + zustand)

The API logic lives in the workspace package `web-ui/packages/pando-client/src`:
- `services/api.ts`: base URL from `window.__PANDO_API_BASE__`, then `VITE_API_BASE_URL`, then same origin; headers; handling of 401 and basic auth.
- `services/sse.ts`: a fetch-stream SSE parser for chat. It drops the token-usage breakdown fields (`sse.ts:234-242`), and `todos_update` is never handled.
- `hooks/useChat.ts`: the streaming state machine (`StreamItem[]`, `ActiveToolCall[]`, steering, reattach, cancel).
- `stores/*`: about 35 stores.
- `types/index.ts`: the best TS reference for SSE payloads.

Features (the native checklist is derived from these):
- **Sessions:**
  - paginated sidebar (100 per page, "load more")
  - client-side search over title and prompt_preview
  - running dot and message count
  - inline rename
  - "New" only clears local state, so the server creates the session on the first prompt
  - no delete or fork in the UI
- **Composer:**
  - 8000-character limit; Enter sends, Shift+Enter inserts a newline
  - slash-command menu from `/api/v1/commands`
  - while streaming, Send becomes **steer**, and steered text shows as "Queued" chips
  - Stop button
  - context meter (warn at 70%, danger at 90%)
  - model chip
- **Messages:**
  - assistant turns are grouped
  - tool and thinking items collapse into an "ActivityGroup" ("Ran 3 commands · read 2 files")
  - per-tool renderers: bash (`$ cmd` + output), edit (diff + counts), read/view (truncated to 2000 chars), write (content), grep/glob/fetch (summary), image thumbnails, generic JSON fallback
  - Markdown with GFM and highlight.js; code-block copy button
  - routing rows, context-filter notices and resurrection/conclusion notice rows
  - auto-scroll with a "scroll to bottom" pill
- **Diffs:**
  - `FileChangesBar` with +/- counts per file, fed by `tool_result.diff`, `raw_output.metadata.files_changed` and agentvcs
  - a Monaco diff viewer that resolves content from agentvcs blobs, the current file with edits reverted, or the tool snippets
- **Permission dialog:**
  - modal; first request shown with a "+N queued" note
  - Allow / Allow for session / Deny
  - escalation styling (`execute_unsandboxed`: shows the command, the justification, and the grant scope from `grant_key`)
- **Question dialog:** a stepper; single or multi select; free-text "Other"; a review step; cancel.
- **Plan banner** from `plan_update`. It persists after the run ends.
- **Goal banner:** objective, status, iteration n/max, progress, next step, cancel.
- **Model switcher:** searchable, grouped by provider, Auto first, badges, session scope.
- **Persona selector** shown in the header.
- **Auto-approve toggle:** StatusBar or Shift+Tab; optimistic update.
- **Info sidebar:**
  - token breakdown and cost
  - cwd and version
  - subagent count (polls tasks every 5 s)
  - plan, LSPs, modified files
- **Projects:**
  - CRUD, activate, init, stop
  - project tabs, each an iframe of `/api/v1/projects/{id}/web/`; switch with Ctrl+Alt+1..9
- **Orchestrator view:** task list and detail (tool calls + output), cancel, delete, cron jobs. It polls.
- **Notifications:** toasts from the notifications SSE; an OS notification "agent done" when the window is in the background.
- **Other views:**
  - logs (polled every 5 s)
  - agentvcs / snapshots: commit log, diff, revert, revert files
  - terminal (xterm over the PTY WebSocket)
  - file explorer + Monaco editor
  - settings (23 categories)
  - instances: remote sessions from other Pando processes
  - design studio
  - setup wizard
- **Keyboard:** Ctrl+P / Ctrl+O quick menu, Ctrl+B sidebar, Ctrl/Cmd+Shift+L theme.
- **Not present:** voice, attachments, @file mentions, a todos panel, live logs.

### 4.2 Wails desktop: `desktop/` + `internal/desktop/` + `cmd/desktop.go`

- `pando desktop` starts the API server in-process (http, port 8765 with fallback). It then execs the embedded wrapper `pando-desktop --url <base>` (`internal/desktop/launcher.go:148-244`, `desktop/main.go:21-90`).
- The wrapper is a **frameless webview that loads the same web UI**. It shows a loading page, polls `/health`, then navigates.
- It has no separate frontend and **does not inject a token**: the web UI does its own `/api/v1/token` exchange.
- Go-side extras (`internal/desktop/app.go`):
  - **Tray** (not on macOS): show, settings, project workspaces, quit (`desktop/tray_systray.go`).
  - **App menu** accelerators: Ctrl+K show, Ctrl+H hide, Ctrl+M simple mode, Ctrl+Shift+D Design Studio. **No OS-global hotkey.**
  - **OS notifications** from `/api/v1/notifications/stream`, only when unfocused and only for `source` agent or tool (`internal/desktop/notifications.go`). **Bug:** it sends no token, so it loops on 401 forever (`notifications.go:58-63`).
  - Native save dialog; open in browser.
  - `SyncShellState` sets the window title and the tray project list.
  - Multi-project = **a separate `pando desktop --cwd` process per project** (`handlers_projects_desktop.go:43-90`, `internal/desktop/workdir.go:121-139`).
  - Design artifacts open in new windows.

### 4.3 SDKs

The SDKs live in `sdk/{typescript,python,java,dotnet}`.
- Their HTTP clients are **stale**: Bearer auth, a non-existent `POST /sessions`, `/sessions/{id}/messages`, and a wrong `models/active` body.
- Their ACP clients use non-standard methods (`prompt/send`).
- Their AG-UI clients are correct for `/api/v1/agui`.

Do not use them as a protocol reference. Use `handlers_chat.go` and `web-ui/packages/pando-client`.

---

## 5. Decision model (`/v1/systemone`) as Pando uses it

Client: `internal/llm/systemone/client.go`.

**Request.** `POST {effectiveBaseURL}/v1/systemone`, `Content-Type: application/json`, and `Authorization: Bearer <apiKey>` when a key is set. Extra `headers` from the config are sent too.
```json
{"model":"tev1:0.8b",
 "state": "<string or JSON object; Pando sends a JSON-object string>",
 "questions": {
   "intent": {"type":"choice","instructions":"…","criteria":{"control":"desc or null","conversation":null,"dictation":"…"}},
   "urgency": {"type":"score","instructions":"…", "...extra type-specific keys"}
 },
 "keep_alive":"30m"}
```
- `questions` is a **map keyed by question name** (1-64 entries).
- `criteria` is an **ordered object**, and the order matters (`client.go:66-126`). A choice question needs 2-26 criteria.
- Types: `choice`, `noul` (yes/no) and `score`.
- Limits: body ≤ 64 KiB (`MaxBodyBytes`); default timeout 30 s. Pando's config defaults are 1500 ms for Ollama and 3000 ms for remote providers.

**Response:**
```json
{"model":"…","answers":{"intent":{"type":"choice","choice":"control","probabilities":{"control":0.93,"conversation":0.05,"dictation":0.02},"confidence":0.88,"score":null}},
 "usage":{"input_tokens":123,"output_tokens":1,"cost":null}}
```
- `checkAnswers` rejects answers that are missing or whose choice is not among the criteria (`client.go:252-275`).
- Errors are typed: unreachable, timeout, unauthorized, too large, malformed (`errors.go`).

**Providers:**
- `ollama`: default base is the configured Ollama provider or `localhost:11434`. Lists models via `/api/tags` with the `decision` capability; needs Ollama ≥ 0.35.
- `typesafe`: `https://api.typesafe.ai`, key from `TYPESAFE_API_KEY`.
- `custom`: any compatible gateway (`internal/config/decision_model.go:17-91`).

**State:** Pando builds it with `modelrouter.BuildState`, a JSON-object string budgeted by the model context (`internal/llm/modelrouter/engine.go:248-270`).

**Health probe:** a `state:"ping"` with a single yes/no question (`provider.go:134-148`).

**For the assistant:**
1. `GET /api/v1/config/decision-model` gives the provider, base and model.
2. `GET /api/v1/decision-model/router/health` gives the availability chip.
3. Call `{effectiveBaseURL}/v1/systemone` directly. That is fine for Ollama, and for TypeSafe when the key is available from the environment.

A masked literal key is unusable, which is gap G6.

---

## 6. Compatibility, versioning and gaps

### 6.1 Compatibility concerns

- **No API version or capabilities endpoint.** `/api/v1/version` reports only the build version. Gate features by probing (for example, 404 on a route) or by a semver minimum. The planned `[DecisionModel]` and `/decision-model/router/*` routes were renamed recently; the deprecated aliases last "one release" (`routes.go:231-237`).
- **Casing is mixed:**
  - the request uses camelCase `sessionId`
  - SSE uses camelCase in `session`/`run` and snake_case elsewhere
  - session and message JSON is PascalCase
  - models and decision JSON is camelCase

  Use per-type serde renames, and `#[serde(default)]` everywhere. Payloads are additive and fields appear over time.
- **Untagged message parts** make history parsing heuristic, and fragile if new part types arrive.
- **TLS:** serve and app use https with a self-signed CA; desktop uses http. The registry does not tell you which.
- **The token is ephemeral:** refetch it on 401 (on loopback `/api/v1/token` always works). A server restart changes both the port (fallback) and the token.
- **The registry path `/tmp/pando-instances` is hard-coded** (`registry.go:14`). It does not follow `$TMPDIR` and is not portable to Windows.
- **There is no keep-alive on SSE.** Idle streams can be silently cut by intermediaries (not an issue on loopback). Use a read-idle watchdog only together with `/pending` polling.
- The orchestrator and per-session state are in memory per process. Multiple Pando processes on the same project share the SQLite DB, but each one's **bgRunner buffers and permission queues are local**. Talk to the process that runs the session.

### 6.2 API gaps: proposed Pando-side tasks

| # | Gap | Why the assistant needs it | Proposal |
|---|---|---|---|
| G1 | Tasks have no push events; only the projected `TaskResponse` | cell birth and death, low latency, no 1 s polling | `GET /api/v1/orchestrator/events` (SSE) built on the existing orchestrator event log (`UnseenEvents`, `RecentEvents`, `SubscribeCompletions`, `orchestrator.go:322-349,607`). Events: `task_created\|status_changed\|progress\|tool_call\|completed` with the full task |
| G2 | `TaskResponse` drops `work_dir, tags, error, exit_code, output_tail, acp_session_id, acp_mode, acp_status, parent_session_id, project_id/name/path, conclusion, dependencies, started_at, completed_at`; `tokens` is always 0 | project mapping, Waiting state, "talk to it", speaking conclusions | add these as additive fields (or `?full=1` returning `models.Task`) |
| G3 | No structured conclusion over HTTP; `conclusion_*`/`resurrected` SSE carry no task id or summary | speak `summary`/`follow_up` and show status and confidence | include `task_id` and `conclusion` in `conclusion_queued`/`conclusion_injected`/`resurrected` payloads, plus `GET /orchestrator/tasks/{id}` → `conclusion` |
| G4 | No pause or resume over HTTP | the voice actions "pausa" and "reanuda" | `POST /orchestrator/tasks/{id}/pause`; `POST /orchestrator/tasks/{id}/resume {prompt, model?}` (returns the new task); also expose the existing `Retry`/`Relaunch` (`orchestrator.go:1525,1628`) |
| G5 | No global session-activity stream; resurrected runs are only discoverable by polling `run_seq` | the organism must react when mesnada wakes up on its own | `GET /api/v1/sessions/events` (SSE): `run_started {sessionId, runSeq, kind: user\|resurrected\|steer}`, `run_finished`, `pending_changed` (permission/question raised or resolved, across all sessions), `session_created\|updated\|deleted` |
| G6 | No decision proxy; API key masked | the assistant cannot use TypeSafe or keyed gateways without duplicating secrets | `POST /api/v1/decision-model/decide {state, questions}` → the systemone `Response`, reusing Pando's timeout and keep-alive, fail-open semantics, 64 KiB cap |
| G7 | Chat request is text only | images, files, screenshots from the desktop | `attachments:[{name, mime, data(base64) \| path}]` on `chat/stream` (the internal `BinaryContent` already exists; `ModelInfo.supportsAttachments` already exists) |
| G8 | No explicit session creation; cannot create, title or select a model before the first prompt | the voice target "new conversation in project X" | `POST /api/v1/sessions {title?, model?}` → `{session}` |
| G9 | No SSE event ids or resume cursor | precise reattach without resetting the bubble | add `id: <runSeq>.<eventIndex>` and honour `Last-Event-ID` against the 500-event ring |
| G10 | No API/protocol version or capabilities | compat gating for a separate Rust client | `GET /api/v1/capabilities` → `{api: 1, features: ["steer","goal","plan_update",…]}` |
| G11 | Message and session JSON has no tags and is PascalCase | robust typed parsing | a versioned DTO (`?format=v2`) with `type`-tagged parts and snake_case keys |
| G12 | `pending` lists only one session | global "N agents waiting for you" badge | `GET /api/v1/pending` across sessions, including ACP task permission (`acp_status.mode`) |
| G13 | Instance discovery: no scheme or token in the registry; `/tmp` path hard-coded | robust discovery on all OSes | add `scheme`/`base_url` to `instanceregistry.Entry`; use `os.TempDir()` or the user runtime dir |
| G14 | No final-message event | knowing when to speak without heuristics | emit `message_complete {messageId, text}` on `AgentEventTypeResponse` (currently swallowed, `handlers_chat.go:711-712`); also expose `MessageID` on deltas (it exists on `AgentEvent`, `agent.go:233-238`) |
| G15 | Token rotation races; no persistence | multi-client stability | guard `s.token` with a mutex (`handlers_extras.go:64`); optionally persist it in `~/.config/pando/api-token` (0600) like the mcp/agui tokens |
| G16 | Remembrances events and memory only via MCP | "¿qué hicimos ayer?" voice queries | REST `POST /remembrances/events/search`, `/memory/recall` |
| G17 | Web UI bugs found in passing | – | `CreateTaskDialog` sends `{name, description}` instead of `prompt` (400); the desktop notifier sends no token (401 loop, `internal/desktop/notifications.go:58-63`); the SSE parser drops the token-usage breakdown (`sse.ts:234-242`); stale `GetServerInfo` binding and `desktop/README.md` |

---

## 7. Native client feature checklist (from web UI + Wails parity + assistant needs)

### 7.1 Connection
- [ ] Registry discovery (`/tmp/pando-instances`), `/health` probe (https then http), manual URL override.
- [ ] Trust `~/.config/pando/tls/ca.crt`.
- [ ] Token via `/api/v1/token`; Basic-auth login dialog for remote servers; refetch on 401.
- [ ] `X-Pando-Token` header and `X-Pando-Client: pando-assistant`.
- [ ] Health polling: 30 s connected, 5 s disconnected. Version display and update hint.
- [ ] Multi-project via `/api/v1/projects` and the child proxy `/api/v1/projects/{id}/web/…`.

### 7.2 Sessions
- [ ] Paginated list (`limit`/`offset`, `has_more`), search, running indicator, message count, `prompt_preview`.
- [ ] New (lazy: created on first prompt), rename (PATCH), delete (DELETE).
- [ ] History loading with the PascalCase/untagged part mapper; fold tool results into tool calls.

### 7.3 Chat streaming
- [ ] `POST /chat/stream`, with a full SSE parser covering every event in §3.3.
- [ ] `run_seq` reattach loop, `/pending` poll every ~4 s, replay reset, history reload fallback.
- [ ] Cancel (also cancel the goal); steer while busy with "queued" chips; `steering_injected` splits the bubble.
- [ ] Thinking display (collapsible).
- [ ] Markdown rendering (GFM, code highlight, copy).

### 7.4 Tool calls
- [ ] Cards keyed by `id` with `kind`/`title`/`status`/`locations`; activity-group collapse.
- [ ] Renderers: execute (command + output + exit), edit (inline diff), write, read, search/glob/grep, fetch, images, generic JSON fallback.
- [ ] File-changes bar + diff view (agentvcs blobs).

### 7.5 Human in the loop
- [ ] Permission dialog with a queue: allow / allow_session / deny; escalation variant with `justification` and `grant_key` scope.
- [ ] Question dialog: multi-question, single or multi select, "Other" text, cancel.
- [ ] Auto-approve toggle per session.
- [ ] Voice mapping: approve/deny/answer by voice. **Never auto-approve when `require_explicit_approval` or `never_auto_approve` is set.**

### 7.6 Plan, todos, goal, usage
- [ ] Plan banner from `plan_update`; todos from `todos_update`.
- [ ] Goal banner + cancel; `/goal` via slash command.
- [ ] Token meter (estimated vs confirmed), cost, cache and reasoning breakdown.
- [ ] Routing and context-filter notices from `system_message`.

### 7.7 Composer
- [ ] Slash-command palette (`/api/v1/commands`).
- [ ] Model switcher (`/models`, `PUT /models/active` with `sessionId`); Auto entry with router health.
- [ ] Persona selector (optional).
- [ ] Dictation insertion from voice (the decision layer's `dictation` intent).
- [ ] @file mentions via `/api/v1/files/search` (beyond the web UI).

### 7.8 Orchestrator and agents (organism cells)
- [ ] Task list polling (1 s until G1 lands); create, cancel, delete; detail with tool calls and output.
- [ ] Map `agent` (= engine) and `status` to cells. Waiting state needs G2/G12; until then, use `/sessions/{id}/pending` for the primary session only.
- [ ] Delegation metrics chip (optional).
- [ ] Pause/resume hidden until G4.

### 7.9 Events
- [ ] Notifications SSE: toasts, plus OS notifications when unfocused. Projects SSE. Config SSE, used to refresh the decision-model chip and models.

### 7.10 Voice and decision
- [ ] Read decision config, show the health chip, call `/v1/systemone` directly (or G6). Fail open.
- [ ] Speak: accumulated `content_delta` at `done`, or a parsed `[delegated-result]` summary.

### 7.11 Desktop shell (Wails parity)
- [ ] Tray, window title per project, OS notifications when unfocused, open in browser, save dialog.
- [ ] Global push-to-talk hotkey (the Wails app has none).
- [ ] Mini window.

### 7.12 Deferred (open the web UI instead)
- Settings, providers, MCP, LSP, skills, setup wizard, design studio, terminal, file editor, logs, snapshots, cron, evaluator.
