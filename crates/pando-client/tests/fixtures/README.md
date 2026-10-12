# pando-client fixtures

Real captures were recorded with `../scripts/record-fixtures.sh` against **Pando v1.3.0**
(`pando serve`, https with the local CA, scratch project dir). Re-run the script to refresh
them; the API token is scrubbed to `<TOKEN>` and never stored.

| File | Source | Notes |
|---|---|---|
| `health.json`, `token.json`, `models.json` (first 4 models), `sessions_list.json`, `session_detail.json`, `session_pending.json`, `orchestrator_tasks.json` | real | |
| `chat_simple.sse` | real | `session, run, system_message*, content_delta, token_usage, goal_status, done` |
| `chat_tool_read.sse` | real | tool_search/read tool calls and results |
| `chat_slash.sse` | real | prompt `/help` (was answered by the model, not as a slash command) |
| `chat_goal_status.sse` | real | `/goal` slash command: no `done` event |
| `session_reattach.sse` | real | `GET /sessions/{id}/stream` replay of the run above |
| `synthetic_events.sse` | **synthetic** | Hand-written from `docs/research/pando-api.md` section 3.3 for events not captured: permission, question, plan/todos, edit diff, steering, conclusion, resurrected, goal, unknown event, error. |

Recorded live later (Pando v1.3.0, scratch project with `[Permissions] AutoApproveTools = false`
and `[Mesnada.Delegation] Enabled/InjectIntoLiveLoop/ResurrectIdleLoop = true`):

| File | Notes |
|---|---|
| `chat_permission.sse`, `session_pending_permission.json` | bash `permission_request`, approved with `POST /permissions/respond {action:"allow"}` |
| `chat_permission_deny.sse` | same, `deny` -> `tool_result` "Permission denied" |
| `chat_question_steer.sse` | `AskUserQuestion` -> `question_request`; a `POST /steer` mid-run yields `steering_injected` (no `steering_queued` was emitted) |
| `chat_cancel.sse` | `POST /cancel` mid-text: `error {"error":"request cancelled by user"}` then `done` (content_delta trimmed to 30) |
| `chat_cancel_while_permission.sse` | cancel while blocked on a permission does NOT end the run until the request is answered |
| `chat_delegate_spawn.sse`, `session_resurrected.sse` | `mesnada_spawn_agent` then idle parent; the reattach replay starts with `run` + `resurrected` |
| `chat_conclusion_injected.sse` | parent still running: `conclusion_injected` (no `conclusion_queued` seen) |
| `chat_edit_diff.sse` | `edit` tool result with `diff {file_path, old_string, new_string}` and `raw_output.metadata.diff` |

`synthetic_events.sse` remains for the shapes still not captured (plan/todos, goal, `steering_queued`,
`conclusion_queued`).
