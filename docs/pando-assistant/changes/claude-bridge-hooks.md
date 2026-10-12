---
created_at: 2026-10-12T07:59:01.88072342Z
updated_at: 2026-10-12T07:59:01.88072342Z
tags:
    - change
    - claude-bridge
---
# claude-bridge: hook receiver, installer, cells, messaging (PDA-T-0130/0131/0132/0133)

Builds on [[pando-assistant/plans/backlog-execution_plan.md]]; interop background in docs/05-claude-code-interop.md (PDA-SP-0010).

## What
- `receiver`: loopback axum server `POST /claude/hook`. Random port, shared secret in header `X-Pando-Hook-Secret` (constant-time check in middleware BEFORE the body is read -> 401), body cap 256 KiB (413), bounded queue 256 (503), unknown events 202 (ignored), bad JSON 400.
- `runtime`: dir 0700, `claude-bridge.port` (port/pid) and `claude-bridge.headers` (secret as a curl header file, 0600, atomic write) so the secret never appears in `ps`.
- `installer`: idempotent merge of one `command` handler per event (SessionStart, UserPromptSubmit, PreToolUse, Notification, Stop, SessionEnd) into a settings.json given by explicit path; timestamped backup before each change, atomic write, follows symlinks, refuses invalid JSON, uninstall removes only our handlers. Generates POSIX forwarder `pando-claude-hook.sh` (curl, connect 1 s / max 2 s, always exit 0, forwards CLAUDE_CODE_MESSAGING_SOCKET/TOKEN as headers). Windows forwarder not done.
- `cells`: pure `SessionTracker::apply(HookEvent, now_ms) -> Vec<CellUpdate>` (Born/Changed/Died/Speak), project by longest cwd path prefix, avísame flag (one-shot, spoken on Stop), inactivity expiry, label lookup for voice.
- `messaging`: `send_message(&MessagingEndpoint, text)` over UDS, optional auth line, 1M char cap, `Delivery::{Sent,SentWithReply,Held}` / `SendError::{Unreachable,Refused,Closed,TooLong,Empty,Unsupported}`; reply classification is heuristic and isolated in `v1`.

## Sources
code.claude.com/docs/en/hooks (2026-10-12): settings schema event -> matcher groups -> handlers {type, command, timeout}; common fields session_id, cwd, hook_event_name; SessionStart `source`/`session_title`; Notification `message`/`notification_type`; Stop `last_assistant_message`; UserPromptSubmit `prompt`. SessionEnd `reason` not confirmed in the pages read (parsed optionally). Docs also offer a native `http` hook type; not used because the port is random and the secret must come from a file. CLAUDE_CODE_MESSAGING_SOCKET in hooks comes from docs/05 (not re-confirmed in the hooks page).

## Verification
`cargo test -p claude-bridge` 26 pass (temp dirs only, real ~/.claude never touched), clippy -D warnings, fmt, `cargo xtask check-deps` OK. Not verified: delivery to a real Claude Code session; a real Claude Code firing the forwarder.

## Gaps
No PostToolUse hook: after a permission prompt is approved the cell stays Waiting until the next PreToolUse/Stop.
