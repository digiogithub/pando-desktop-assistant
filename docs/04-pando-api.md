# 04 · Pando API surface used by the assistant

Everything below was read from `../../pando/internal/api/routes.go`, `handlers_*.go`,
`pkg/mesnada/models/task.go` and `internal/mesnada/conclusion/` on 2026-10-11. Re-check
before coding; the API is versioned under `/api/v1` and served by `pando serve`.

Auth: bearer token of the API server (`POST /api/v1/config/api-server/regenerate-token`
rotates it). **[verify]** where the token is persisted so the assistant can read it
(`~/.pando/...` or the project `.pando/`).

## Discovery

| Endpoint | Use |
|---|---|
| `GET /health`, `GET /api/v1/version` | liveness, version gate |
| `GET /api/v1/project`, `GET /api/v1/project/context` | current project |
| `GET /api/v1/projects`, `GET /api/v1/projects/events` (SSE) | registered projects (the "which project" choice list) |
| `GET /api/v1/instances` | running Pando instances: `instance_id, path, pid, pub_port, rpc_port, web_port, started_at, mode, parent_instance_id, is_primary` — warm per-project instances become cells too |
| `GET /api/v1/models`, `PUT /api/v1/models/active` | header chip |

## Conversation (the "talk to mesnada" path)

| Endpoint | Use |
|---|---|
| `GET/POST /api/v1/sessions`, `/api/v1/sessions/{id}` | list/create/select the session the assistant speaks into |
| `POST /api/v1/chat/stream` | send a prompt; SSE response |
| `GET /api/v1/sessions/{id}/stream` | reattach to a running session |
| `POST /api/v1/sessions/{id}/steer` | inject a mid-run instruction ("steering") — voice "y además…" |
| `POST /api/v1/sessions/{id}/cancel` | stop |
| `GET /api/v1/sessions/{id}/pending` | pending permission/question |
| `POST /api/v1/permissions/respond`, `POST /api/v1/questions/respond` | voice "aprueba" / "deniega" / answer |
| `GET/POST /api/v1/sessions/{id}/auto-approve` | toggle |

SSE event names emitted by the chat stream (`handlers_chat.go`):
`session`, `thinking_delta`, `content_delta`, `tool_call`, `tool_result`, `todos_updated`,
`token_usage`, `system_message`, `steering_queued`, `steering_injected`,
`conclusion_queued`, `conclusion_injected`, `resurrected`, `response`, `error`.

Organism mapping: `thinking_delta` → `thinking`; first `content_delta` → stop thinking;
`tool_call` → nucleus satellite blip; `response` → speak; `conclusion_*` → wave from the
cell that concluded; `resurrected` → body bulge toward that cell.

## Agents = orchestrator tasks (mesnada)

| Endpoint | Use |
|---|---|
| `GET /api/v1/orchestrator/tasks` | the cell list (poll 1 s, or subscribe to notifications) |
| `POST /api/v1/orchestrator/tasks` | spawn: `{prompt, work_dir?, model?, engine?, tags?, dependencies?, background}` |
| `GET /api/v1/orchestrator/tasks/{id}` | detail for the side panel |
| `POST /api/v1/orchestrator/tasks/{id}/cancel`, `DELETE …/{id}` | voice "cancela" |
| `GET /api/v1/orchestrator/delegation/metrics` | telemetry chip (optional) |

Task JSON (from `models.Task`): `id, prompt, work_dir, status, engine, pid, output,
output_tail, raw_error, error, exit_code, model, log_file, progress{percentage,
description, updated_at}, created_at, started_at, completed_at, dependencies, tags,
priority, persona, acp_session_id, acp_mode, acp_status{session_id, mode, agent_name,
is_connected, tool_calls, last_activity}, current_tool, tool_calls[], parent_session_id`.

`status` ∈ `pending | running | paused | completed | failed | cancelled`.
`engine` ∈ `copilot | claude | gemini | opencode | ollama-claude | ollama-opencode | mistral |
acp | acp-claude | acp-codex | acp-custom | acp-server | pando | warm-acp` plus custom
engines from `.pando/mesnada/engines/*.template.yaml` (`output_format: text | jsonl`).

There is no pause endpoint in the HTTP API today (status `paused` exists in the model).
**[verify]** — if absent, the voice action `pause` maps to the MCP tool side or is hidden.

## Conclusions (what gets spoken)

Subagents end with a sentinel block; the **last** block wins and the body is YAML:

```
<pando:conclusion>
status: done            # done | partial | blocked | failed
summary: API estable, sin regresiones
artifacts: [internal/api/handlers_chat.go]
memory_refs: []
follow_up: ""
confidence: 0.9
</pando:conclusion>
```

Pando enriches it with task id, engine, model, project and parent session. The assistant
speaks `summary` (+ `follow_up` if present) and shows `status`/`confidence` in the cell card.
Parser: `internal/mesnada/conclusion/parse.go` (tolerant; warnings never discard data).

## Events

| Endpoint | Use |
|---|---|
| `GET /api/v1/notifications/stream` (SSE, `event: notification`) | task created/finished, permission requests — primary trigger for cell birth/death |
| `GET /api/v1/config/events` (SSE) | config changes (decision model switched, etc.) |
| `GET /api/v1/logs/stream` | optional debug pane |

## Decision model

`GET/PUT /api/v1/config/decision-model`, `DELETE …/api-key`,
`POST /api/v1/decision-model/router/test` (lets the assistant test its question set against
the configured model). See 03.

## Other protocols Pando speaks (not used in v1, good to know)

- **ACP** (`pando acp`, stdio) — Pando itself can be an ACP server for editors; mesnada also
  *consumes* ACP engines (`acp-claude`, `acp-codex`) and can reuse warm per-project ACP
  instances (`docs/delegation.md`).
- **AG-UI** (`pando agui-serve`) — browser-oriented side-car; not needed for a native client.
- **IPC bus** on localhost between instances ("hot-peer delegation", `instance.ping`).
