---
id: PDA-SP-0001
type: spec
title: Connection and discovery of Pando
status: backlog
author: mcp
labels: [pando-client]
created: 2026-10-11T21:41:32Z
updated: 2026-10-11T22:02:09Z
requirements:
  R1:
    status: backlog
  R2:
    status: backlog
  R3:
    status: backlog
  R4:
    status: backlog
  R5:
    status: backlog
  R6:
    status: backlog
---

## Purpose
How the assistant finds a Pando instance, authenticates, and keeps the connection alive, without the user configuring anything in the local case.

## Scope
Crate `pando-client` (based on `pando-rs` from Bitácora): instance registry `/tmp/pando-instances/*.json`, probe `/health` (https, then http), local CA `~/.config/pando/tls/ca.crt`, ephemeral token via `/api/v1/token`, headers `X-Pando-Token` + `X-Pando-Client`, connection state, multi-project via proxy `/api/v1/projects/{id}/web/…`. Out: Pando process management (managed mode).

Refs: `docs/research/pando-api.md` §1, §6.1.

## Requirements

### PDA-SP-0001.R1 — Automatic discovery of local instance

The client SHALL discover a running Pando HTTP API without user configuration by reading the instances registry (`/tmp/pando-instances/*.json`, entries with `web_port > 0`), preferring `is_primary`, probing `GET /health` with `https` first for `mode=webui` and `http` for `mode=desktop`, and falling back to `https://localhost:8765` then `http://localhost:8765`. A manual base URL in settings SHALL override discovery.

#### Scenario: pando serve running
- GIVEN `pando serve` registered with `web_port 8765`, mode `webui`
- WHEN the assistant starts
- THEN it connects to `https://localhost:8765` and shows the project name from `/health`

#### Scenario: no instance
- GIVEN no registry entry answers `/health`
- WHEN the assistant starts
- THEN connection state is `Unavailable` with a hint to run `pando serve` and retries with backoff

### PDA-SP-0001.R2 — TLS with the local Pando CA

The client SHALL trust `$XDG_CONFIG_HOME/pando/tls/ca.crt` (default `~/.config/pando/tls/ca.crt`) as an additional root certificate and SHALL NOT disable certificate verification.

#### Scenario: self-signed server
- GIVEN Pando serves https with its auto-generated CA
- WHEN the client calls `/health`
- THEN the TLS handshake succeeds without `danger_accept_invalid_certs`

#### Scenario: CA missing
- GIVEN the CA file does not exist and the server is https
- THEN the state is `Unavailable{reason: tls}` with an actionable message

### PDA-SP-0001.R3 — Ephemeral token and authentication headers

The client SHALL obtain the API token from `GET /api/v1/token` on loopback, send it as `X-Pando-Token` (or `?token=` only where headers are impossible) together with `X-Pando-Client: pando-assistant`, refetch it once on any 401, and fall back to Basic auth credentials from the OS keychain when the server requires `basic_auth_required`. It SHALL NOT call `regenerate-token` and SHALL NOT log the token.

#### Scenario: server restarted
- GIVEN a cached token and Pando restarted with a new token
- WHEN a request returns 401
- THEN the client refetches `/api/v1/token` and retries the request once

#### Scenario: remote server
- GIVEN a non-loopback base URL
- WHEN `/api/v1/token` answers 401 `basic_auth_required`
- THEN the UI asks for credentials once and stores them in the keychain

### PDA-SP-0001.R4 — Connection state machine

The client SHALL expose a connection status `Connecting | Connected{version, project} | Unavailable{reason} | Unauthorized | TooOld{min}` updated by a health probe every 30 s while connected and with exponential backoff (1 s → 30 s) while disconnected, and SHALL gate features on a minimum Pando version (and on `/api/v1/capabilities` when it exists).

#### Scenario: Pando stops
- GIVEN state `Connected`
- WHEN `/health` fails
- THEN state becomes `Unavailable`, the header chip shows it, all SSE streams stop and are re-opened on reconnection

#### Scenario: old Pando
- GIVEN `/api/v1/version` below the minimum
- THEN state is `TooOld` and the update command from `/version` is shown

### PDA-SP-0001.R5 — Robust SSE streams

The client SHALL provide a generic SSE stream over a dedicated HTTP client without overall timeout, verify `Content-Type: text/event-stream`, parse multi-line `data:` and unnamed events, tolerate unknown event names (`Unknown{raw}`), and reconnect notification/project/config streams with exponential backoff (1 s → 30 s). Dropping a stream SHALL NOT cancel a run.

#### Scenario: unknown event
- GIVEN Pando emits a new event name
- THEN the parser yields `Unknown` and the stream continues

#### Scenario: proxy returns HTML
- GIVEN a 200 response with `text/html`
- THEN the stream fails with `Protocol` error, not a parse panic

### PDA-SP-0001.R6 — Multi-project via the primary's proxy

The client SHALL list projects via `GET /api/v1/projects`, follow `/api/v1/projects/events`, and address a project child server through `/api/v1/projects/{id}/web/api/v1/...` on the primary connection (starting it with `POST /projects/{id}/web/open` when needed), so one authenticated connection drives every project.

#### Scenario: switch project by voice
- GIVEN project `bitacora` is stopped
- WHEN the user selects it as the voice target
- THEN the client opens its child server and routes chat calls through the proxy
