---
id: PDA-M-0009
type: milestone
title: M8 · Interop with Claude Code
status: backlog
author: mcp
created: 2026-10-11T21:40:56Z
updated: 2026-10-11T22:02:15Z
due: 2027-03-19
---

## Description
Claude Code sessions opened by the user appear as cells (hooks → local endpoint), one-way messages via a cross-session messaging socket, and a bidirectional `pando-channel` behind a feature flag.

## Acceptance Criteria
- SessionStart/Stop/SessionEnd reflected in the organism in < 500 ms.
- "Envía a api-worker: …" delivers the text to the session (Linux/macOS).
- The hook installer is idempotent and reversible.

## Notes
Refs: `docs/05-claude-code-interop.md`. Feature `claude-bridge`.
