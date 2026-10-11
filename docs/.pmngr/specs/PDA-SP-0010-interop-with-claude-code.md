---
id: PDA-SP-0010
type: spec
title: Interop with Claude Code
status: backlog
author: mcp
labels: [claude-bridge]
created: 2026-10-11T21:41:32Z
updated: 2026-10-11T22:04:42Z
requirements:
  R1:
    status: backlog
  R2:
    status: backlog
  R3:
    status: backlog
  R4:
    status: backlog
---

## Purpose
View and talk to Claude Code sessions that the user opens outside Pando.

## Scope
Crate `claude-bridge` (feature `claude-bridge`): local HTTP hook receiver, hook installer in `~/.claude/settings.json`, cross-session messaging over socket, MCP channel `pando-channel` (v2). Agents launched by Pando: only via the Pando API.

Refs: `docs/05-claude-code-interop.md`.

## Requirements

### PDA-SP-0010.R1 — Local hook receiver

The bridge SHALL listen on `127.0.0.1:<port>/claude/hook` (random port written to a 0600 runtime file, shared-secret header), accept SessionStart, UserPromptSubmit, PreToolUse, Notification, Stop and SessionEnd payloads, and map them to ClaudeExternal cells (born hollow blue; working; waiting with `?`; done with Stop summary; dies on SessionEnd), placing each under the Pando project whose path matches its cwd.

#### Scenario: session opened in a terminal
- WHEN the user starts `claude` in `~/www/bitacora`
- THEN a hollow blue cell labelled with the session name appears under project bitacora in < 500 ms

### PDA-SP-0010.R2 — Hook installer

The app SHALL install, verify and uninstall a user-level hook set in `~/.claude/settings.json` idempotently (merging with existing hooks, backup before write), using a tiny forwarder script that reads the port/secret file and fails silently when the assistant is not running.

#### Scenario: assistant not running
- WHEN Claude Code fires a hook and the assistant is closed
- THEN the hook exits 0 quickly and Claude Code is unaffected

### PDA-SP-0010.R3 — One-way messages over socket

On Linux/macOS the bridge SHALL send plain-text messages to a session's `CLAUDE_CODE_MESSAGING_SOCKET` (captured from SessionStart, with the auth token line when available), within the documented size cap, and report delivery failures ("held", refused, closed) to the user. Anything beyond posting one line SHALL be treated as version-fragile and isolated.

#### Scenario: send by voice
- WHEN the user says "dile a api-worker que el schema cambió"
- THEN the text is delivered to that session's socket and the cell shows a vesicle inward

### PDA-SP-0010.R4 — Bidirectional pando-channel (v2)

Behind feature `claude-bridge-channel`, the assistant SHALL support sessions started with the `pando-channel` MCP channel (served by panorganon) to push messages into the session and receive replies via its `reply` tool, rendering replies in the cell card and speaking them when requested. Messages to Claude SHALL carry the `<pando:conclusion>` convention so results come back parseable.

#### Scenario: reply
- WHEN the session replies through the channel
- THEN the text appears in the card and, if "avísame" was requested, is spoken
