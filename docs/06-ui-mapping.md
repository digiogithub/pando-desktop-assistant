# 06 · From API events to the organism

The organism crate is pure state; this document is the adapter contract between
`pando-client` / `claude-bridge` / voice and `organism::Inputs`.

> Reconciled on 2026-10-12 (PDA-T-0016) with [`research/pando-api.md`](research/pando-api.md)
> (what the HTTP API really exposes) and [`research/organism-port.md`](research/organism-port.md).
> Visual decisions (stable slots, overflow, death, pending dim) are in
> `design-system/README.md` §7 (PDA-T-0017).

```rust
pub struct Inputs<'a> {
    pub state: BodyState,            // Idle | Listening | Thinking | Speaking | Delegating
    pub mic_rms: f32,                // 0..1, already smoothed
    pub tts_rms: f32,                // 0..1
    pub pointer: Option<Point>,      // canvas space
    pub cells: &'a [CellInput],
}
pub struct CellInput { pub id: CellId, pub engine: Engine, pub status: Status, pub born_at: Option<Instant>, pub label: String }
pub enum Engine { Pando, ClaudeAcp, ClaudeExternal, Cli }
pub enum Status { Working, Waiting, Paused, Done, Error }
```

## Body state machine

| From → to | Trigger |
|---|---|
| Idle → Listening | push-to-talk pressed, or VAD opens |
| Listening → Thinking | transcript sent (conversation) — or → Idle if it was a command (commands are < 50 ms) |
| Thinking → Speaking | `done` with accumulated `content_delta` text to speak (there is no `response` event) |
| Thinking → Idle | `done` with nothing to speak (silent tool work), or stream closed without `done` (reattach in background) |
| Speaking → Idle | playback finished |
| Speaking → Listening | barge-in |
| any → Delegating (1.5 s) → previous | a new task id appears in the polled `orchestrator/tasks` diff, `POST orchestrator/tasks` returned, or a mesnada spawn `tool_result` arrives in the chat stream (the notifications stream carries no task events; push needs G1, PDA-T-0155) |

`Delegating` is a transient overlay: it does not cancel Thinking/Speaking, it just adds the
bulge and the birth of the cell, then returns.

## Cell mapping

| Source | id | engine | status |
|---|---|---|---|
| `orchestrator/tasks[]` (`TaskResponse`) | `task.id` (label: `name`, else first 24 chars of `prompt`) | from `agent`: `pando` → `Pando`; `acp-claude`, `warm-acp`, other `acp-*` → `ClaudeAcp`; `claude`, `copilot`, `gemini`, `opencode`, `mistral`, `ollama-*`, custom `text` engines → `Cli` | `pending` → Working (dimmed 60 %, no beat), `running` → Working, `paused` → Paused, `completed` → Done, `failed` → Error, `cancelled` → Done (then dies after 10 s) |
| primary session with non-empty `sessions/{id}/pending` | the body (mesnada), not a cell | — | header badge `?`; the body is the one waiting |
| task waiting on an ACP permission | same | same | **Waiting** — not detectable over HTTP until G2/G12 land (PDA-T-0156, PDA-T-0159) |
| `instances[]` (warm per-project, `is_primary == false`) | `instance_id` | `Pando` | Working (no in-flight data over HTTP; refine when G5 lands) |
| Claude hook `SessionStart` | session name | `ClaudeExternal` | Working / Waiting / Done per hook (05) |

`TaskResponse` omits `work_dir`, `tags`, `output_tail`, `acp_session_id`, `acp_status` and
`conclusion` (gap G2). Everything below that needs them degrades until G2 lands.

Ordering around the ring: **stable slots** by `created_at`; new cells take the next free
slot so existing cells do not jump (positions are springs, so any compaction is still smooth).

## Selection → side card

Selecting a cell shows: label, engine label, status label, project (from `work_dir` matched
to `projects[]` once G2 exposes it; hidden until then), the last 3 lines of `output` (or the
hook summary), `progress` and `current_tool`, and the actions the engine allows:

| Engine | Actions |
|---|---|
| Pando / ClaudeAcp | Cancel; Talk to it and View conclusion once G2/G3 expose the session id and conclusion (PDA-T-0156, PDA-T-0157) |
| ClaudeAcp waiting | Approve / Deny once ACP permissions are reachable (G12, PDA-T-0159); session permissions go through `permissions/respond` |
| ClaudeExternal | Send message (socket/channel), Notify me when done |
| Cli | View full output, Relaunch (`POST orchestrator/tasks` with same prompt) |

## Voice target

The voice always goes to **mesnada** (the primary session) unless the user selected a cell
that can talk and said "háblale" / clicked *Talk to it*; then prompts go to that task's
session (`acp_session_id`, available only after G2) until "vuelve" / Esc. The header pill
shows the target.

## What is spoken

Priority: `summary` of the `[delegated-result …]` block for the cell the user asked about
(parsed from session history; structured conclusions need G3) > mesnada's reply text
(accumulated `content_delta` until `done`) > hook `Stop` summary when the user asked to be
notified. Never speak tool output. Queue
speech; while the body is Listening or Speaking, new conclusions wait (cells pulse once to
signal "I have something").

## Mini window

Same `Inputs`; `organism::Scene::layout(size)` scales everything by canvas width, so the
118 px version is the same code. Pixel constants scale linearly with minimum stroke widths
(see design-system README §7). Labels and badges are hidden under 200 px.
