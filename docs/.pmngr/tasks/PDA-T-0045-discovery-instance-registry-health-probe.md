---
id: PDA-T-0045
type: task
title: "Discovery: instance registry + /health probe"
status: done
parent: PDA-US-0012
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [pando-client]
estimate: 3
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:25:33Z
started: 2026-10-11T22:25:22Z
closed: 2026-10-11T22:25:33Z
---

## Description
Read `/tmp/pando-instances/*.json` (portable path once G13 exists), filter `web_port > 0`, prefer primary and `desktop`/`webui`, try https/http depending on mode, fallback 8765; manual URL override; URL policy (http/https, no credentials, remote only over https + `allow_remote`).

## Acceptance Criteria
- Tests with a temporary registry directory and mock http/https servers.
