---
id: PDA-US-0040
type: story
title: As a user I want to send messages to a Claude Code session and receive its reply
status: backlog
priority: medium
parent: PDA-EP-0012
milestone: PDA-M-0009
author: mcp
labels: [claude-bridge]
created: 2026-10-11T21:54:52Z
updated: 2026-10-11T22:03:18Z
---

## Description
One-way send via socket of cross-session messaging (v1, Linux/macOS), voice command "dile a <sesión> …", and bidirectional `pando-channel` channel in panorganon behind feature flag (v2).

## Acceptance Criteria
- Implements PDA-SP-0010.R3; R4 after `claude-bridge-channel`.
