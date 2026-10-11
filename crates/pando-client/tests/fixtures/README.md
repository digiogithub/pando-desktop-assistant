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

Not yet recorded from a live server (pending, tracked in PDA-T-0027): permission and question
round trips, steering, cancel, delegation with `conclusion_*`. They need a configured sandbox /
mesnada setup and non-trivial LLM spend; `synthetic_events.sse` covers their wire shapes.
