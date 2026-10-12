---
id: PDA-T-0068
type: task
title: Batched chat pump and prompt sending
status: done
parent: PDA-US-0018
milestone: PDA-M-0004
assignees: [claude-sonnet]
author: mcp
labels: [chat]
estimate: 2
created: 2026-10-11T21:51:26Z
updated: 2026-10-12T08:20:17Z
started: 2026-10-12T08:06:42Z
closed: 2026-10-12T08:20:17Z
---

## Description
Session service on tokio (`ChatHandle` send/steer/cancel usable from any thread) → channel → event pump that drains bursts and applies one batch per render; bind composer and session selection.

## Acceptance Criteria
- Sending a prompt creates/uses a session and streams fully until `done`.
