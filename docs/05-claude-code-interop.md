# 05 · Talking to Claude Code sessions

> Reviewed on 2026-10-12 (PDA-T-0016): Pando-side facts aligned with
> [`research/pando-api.md`](research/pando-api.md); the Claude Code facts below were read
> from code.claude.com on 2026-10-11.

Three kinds of Claude Code processes can appear as cells:

| Kind | Who starts it | How the assistant talks to it | Cell look |
|---|---|---|---|
| mesnada task with engine `claude` (CLI, `-p`) | Pando | read-only: task output | spore (dashed) if the engine is text-only; otherwise solid blue |
| mesnada task with engine `acp-claude` / `warm-acp` | Pando | ACP is handled inside Pando; over HTTP the assistant sees only the projected `TaskResponse` (no `acp_session_id`, no ACP permission state — gaps G2/G12, PDA-T-0156/PDA-T-0159), so v1 can read output and cancel, not talk or approve | solid blue nucleus |
| session the user opened in a terminal | user | **cross-session messaging** or **channels** (below) | hollow blue nucleus |

## Option A — cross-session messaging socket (same machine)

Facts (code.claude.com/docs/en/cross-session-messaging, read 2026-10-11):

- Requires Claude Code ≥ 2.1.224 (macOS/Linux/WSL2), ≥ 2.1.234 native Windows. On by default.
- Each session binds an **inbox socket**: a Unix domain socket (named pipe on Windows), path
  exported to hooks and Bash as `CLAUDE_CODE_MESSAGING_SOCKET`, shown in `/status` → "Peer
  address" (`uds:` prefix). Private per-user dir `/tmp/cc-socks-<uid>` when needed.
- A script can post to a session's socket. First line may be
  `{"type":"auth","token":"<CLAUDE_CODE_MESSAGING_TOKEN>"}` (optional on Linux/macOS,
  required on Windows). Connection is closed if no complete line arrives within 30 s.
- **Plain text only**; ~1 M chars cap; bursts are rate-limited; loops are throttled.
- Inbound controls: `crossSessionInbound` = `accept | hold | refuse` (settings or `/config`).
  With no value, delivery depends on permission classes: a receiver in bypass mode **holds**
  messages from senders it cannot classify until the user approves (5 min `dialogExpiry`).
  A message from a child process the session can verify (its own hook/Bash) is delivered.
- The receiving Claude is told the text came from another session; it cannot approve
  prompts, change config or run slash commands.
- Container ↔ host and WSL2 ↔ Windows cannot reach each other.

**Not documented**: the full line format beyond the auth line, and the registration files
used for discovery. Treat anything beyond "post one JSON/text line to the socket" as
reverse-engineered and version-fragile. `claude -p` workers also bind an inbox
(`crossSessionInbound: accept` in `--settings` lets them take messages unattended).

Use for: one-way nudges from the assistant into a session the user opened ("@api-worker:
el schema cambió"). Prefer sending from a **hook of that session** (own-child path) so it
is delivered without a dialog.

## Option B — channels (documented, two-way, research preview)

A channel is an MCP server that pushes events into a running session; the session replies
through the server's `reply` tool. Started with `claude --channels plugin:<name>@<marketplace>`;
custom ones during preview need `--dangerously-load-development-channels`. Requires
claude.ai or Console auth; Team/Enterprise must enable `channelsEnabled`. Events arrive as
`<channel source="plugin:…">` and only while the session is open.

Plan: ship a `pando-channel` MCP server inside **panorganon** (Pando's MCP proxy) so a
session started with the channel registers itself with the assistant, receives messages from
it and replies through `reply`. Rate of change is high (flag syntax "may change") — keep it
behind the `claude-bridge` feature.

## Discovery and liveness — hooks (the stable part)

Install a user-level hook set (`~/.claude/settings.json`) that the assistant can receive on a
local HTTP endpoint (`127.0.0.1:<port>/claude/hook`):

| Hook | Payload the assistant needs | Organism effect |
|---|---|---|
| `SessionStart` | session id/name, cwd, `CLAUDE_CODE_MESSAGING_SOCKET`, `CLAUDE_CODE_MESSAGING_TOKEN` | cell is born (hollow blue) |
| `UserPromptSubmit` / `PreToolUse` | — | status `working` |
| `Notification` (permission needed) | message | status `waiting`, badge `?` |
| `Stop` | last assistant text (summary) | status `done`, cell nucleus Marfil; speak if the user asked "avísame" |
| `SessionEnd` | — | cell dies |

The hook script is a 10-line shell/`curl` that forwards the hook JSON; the assistant
matches cwd against Pando projects to place the cell under the right project.

## Option C — Agent teams inboxes (`~/.claude/teams/*/inboxes/*.json`)

Experimental, reverse-engineered, reports of undelivered messages. **Do not depend on it.**

## Decision

1. Agents Pando spawns: via Pando's API only (ACP handled inside Pando; see 04 for what the
   HTTP projection exposes).
2. User-opened sessions: hooks for discovery/status (v1), socket for one-way nudges (v1,
   Linux/macOS), channel for two-way (v2, feature-flagged).
3. Pando → Claude Code messages carry the `<pando:conclusion>` convention in the text so a
   Claude session can hand results back in a form Pando already parses.
