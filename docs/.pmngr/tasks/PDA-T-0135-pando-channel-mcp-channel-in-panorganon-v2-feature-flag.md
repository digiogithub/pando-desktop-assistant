---
id: PDA-T-0135
type: task
title: pando-channel MCP channel in panorganon (v2, feature flag)
status: backlog
parent: PDA-US-0040
milestone: PDA-M-0009
author: mcp
labels: [claude-bridge, upstream-pando]
estimate: 5
created: 2026-10-11T21:56:03Z
updated: 2026-10-11T22:02:28Z
---

## Description
MCP channel served by panorganon that registers the session with the assistant, pushes messages and exposes `reply`; the assistant shows replies and speaks them if requested. Messages use the `<pando:conclusion>` convention. Behind the `claude-bridge-channel` feature and docs for `--dangerously-load-development-channels`.

## Acceptance Criteria
- Round trip with a session launched with the channel.
