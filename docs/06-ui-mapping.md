# 06 · From API events to the organism

The organism crate is pure state; this document is the adapter contract between
`pando-client` / `claude-bridge` / voice and `organism::Inputs`.

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
| Thinking → Speaking | `response` / `conclusion_injected` with text to speak |
| Thinking → Idle | response with nothing to speak (silent tool work) |
| Speaking → Idle | playback finished |
| Speaking → Listening | barge-in |
| any → Delegating (1.5 s) → previous | `notification` task created (or `POST orchestrator/tasks` returned) |

`Delegating` is a transient overlay: it does not cancel Thinking/Speaking, it just adds the
bulge and the birth of the cell, then returns.

## Cell mapping

| Source | id | engine | status |
|---|---|---|---|
| `orchestrator/tasks[]` | `task.id` (label: `tags[0]` or first 24 chars of `prompt`) | `pando`, `warm-acp`, `acp-*` → `Pando` if engine is `pando`, else `ClaudeAcp` for `acp-claude`/`acp-*`; `claude`, `copilot`, `gemini`, `opencode`, `mistral`, custom `text` engines → `Cli` | `pending` → Working (dim 60 %), `running` → Working, `paused` → Paused, `completed` → Done, `failed` → Error, `cancelled` → Done (then fade out after 10 s) |
| task with `acp_status.mode == "permission"` or `sessions/{id}/pending` non-empty | same | same | **Waiting** overrides |
| `instances[]` (warm per-project, `is_primary == false`) | `instance_id` | `Pando` | Working if it has in-flight loops else Done |
| Claude hook `SessionStart` | session name | `ClaudeExternal` | Working / Waiting / Done per hook (05) |

Ordering around the ring: stable by `created_at`; new cells are appended at the next free
angle so existing cells do not jump (positions are springs, so a reflow is still smooth).

## Selection → side card

Selecting a cell shows: label, engine label, status label, project (from `work_dir` matched
to `projects[]`), last 3 lines of `output_tail` (or the hook summary), and the actions the
engine allows:

| Engine | Actions |
|---|---|
| Pando / ClaudeAcp | Talk to it (opens its session as the voice target), Cancel, View conclusion |
| ClaudeAcp waiting | Approve / Deny (→ `permissions/respond`) |
| ClaudeExternal | Send message (socket/channel), Notify me when done |
| Cli | View full output, Relaunch (`POST orchestrator/tasks` with same prompt) |

## Voice target

The voice always goes to **mesnada** (the primary session) unless the user selected a cell
that can talk and said "háblale" / clicked *Talk to it*; then prompts go to that task's
session (`acp_session_id`) until "vuelve" / Esc. The header pill shows the target.

## What is spoken

Priority: conclusion `summary` of the cell the user asked about > mesnada's `response` text
> hook `Stop` summary when the user asked to be notified. Never speak tool output. Queue
speech; while the body is Listening or Speaking, new conclusions wait (cells pulse once to
signal "I have something").

## Mini window

Same `Inputs`; `organism::Scene::layout(size)` scales everything by canvas width, so the
118 px version is the same code. Labels and badges are hidden under 200 px.
