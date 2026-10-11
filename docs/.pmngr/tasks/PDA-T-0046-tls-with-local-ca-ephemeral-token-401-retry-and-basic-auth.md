---
id: PDA-T-0046
type: task
title: TLS with local CA, ephemeral token, 401 retry and Basic auth
status: backlog
parent: PDA-US-0012
milestone: PDA-M-0003
author: mcp
labels: [pando-client, security]
estimate: 3
created: 2026-10-11T21:50:04Z
updated: 2026-10-11T22:03:06Z
---

## Description
Add `~/.config/pando/tls/ca.crt` as a root; `GET /api/v1/token`; middleware that refetches the token once on 401; if `basic_auth_required`, credentials from the keychain (`keyring`, service `pando-assistant`) with a single dialog; never log the token or passwords.

## Acceptance Criteria
- Test: server restart with a new token ⇒ request retried successfully.
- Test that logs contain no secrets.
