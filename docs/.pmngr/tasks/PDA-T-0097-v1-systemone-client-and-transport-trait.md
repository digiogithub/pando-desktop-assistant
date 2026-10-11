---
id: PDA-T-0097
type: task
title: /v1/systemone client and transport trait
status: backlog
parent: PDA-US-0028
milestone: PDA-M-0006
author: mcp
labels: [decide]
estimate: 3
created: 2026-10-11T21:52:55Z
updated: 2026-10-11T22:03:24Z
---

## Description
`decide` defines `trait DecisionTransport`; HTTP implementation (in `app` or `pando-client`) with base/model/keepAlive from `/config/decision-model`, `Authorization` if a key is set in the environment, 400 ms timeout, typed errors (unreachable, timeout, unauthorized, too large, malformed); prepared for proxy G6.

## Acceptance Criteria
- Tests with a mock server that reproduces Ollama responses and errors.
