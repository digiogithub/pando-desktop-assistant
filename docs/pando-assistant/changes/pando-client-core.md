---
created_at: 2026-10-11T22:30:42.602348271Z
updated_at: 2026-10-11T22:30:42.602348271Z
tags:
    - change
    - pando-client
---
# pando-client: fixtures, core, discovery, auth, ConnectionService

Part of [[pando-assistant/plans/backlog-execution_plan.md]]. Tasks: PDA-T-0027, PDA-T-0044, PDA-T-0045, PDA-T-0046, PDA-T-0047.

## What changed
- **T-0027**: `crates/pando-client/scripts/record-fixtures.sh` starts `pando serve` (v1.3.0, scratch git dir, port 18765), fetches the loopback token and records real SSE/REST bytes into `crates/pando-client/tests/fixtures/` (token scrubbed). `synthetic_events.sse` (clearly marked in `tests/fixtures/README.md`) covers permission, question, plan/todos, diff, steering, conclusion, resurrected, goal, unknown event, error. Not captured live: permission/question round trips, steering, cancel, delegation (need sandbox/mesnada + spend). T-0027 left in_review for that.
- **T-0044**: `src/{client,config,error,sse}.rs` adapted from Bitacora `pando-rs` (MIT): two reqwest clients (normal + stream with idle timeout only), `PandoConfig` with local CA, typed `Error`, redacted `Token`/`BasicAuth`, `SseParser` returning named events (`SseEvent`), `SseStream`. Header `X-Pando-Client: pando-assistant`. Default stream idle timeout is 300 s because Pando sends no SSE keep-alives.
- **T-0045**: `src/discovery.rs`: registry reader (`/tmp/pando-instances`), candidate ordering (primary, desktop/webui, children last; http-first for desktop), manual URL, URL policy (`validate_url`), `/health` probe.
- **T-0046**: `src/auth.rs` + recovery in `PandoClient::send`: single-flight token refetch on 401, Basic credentials from keychain (`SecretStore`, `KeyringStore`, service `pando-assistant`) or one login dialog (`CredentialPrompt`), sensitive headers.
- **T-0047**: `src/connection.rs`: `ConnectionService` (watch-based `Snapshot`, `Link` for coordinated stream close/reopen by generation, `nudge()`), states `Connecting|Connected|Unavailable|Unauthorized|TooOld`, 30 s health, 1 to 30 s backoff, lenient `Version` gate.

## Files and symbols
`crates/pando-client/src/*`, `tests/{client_core,discovery,auth,no_secret_logs,connection,live}.rs`, `tests/common/mod.rs` (axum http/https servers with an rcgen CA), workspace `Cargo.toml` (reqwest, serde, thiserror, axum, keyring).

## Why
Foundation for chat and the decision layer (PDA-US-0012); built on the existing WebUI REST+SSE API (see [[pando-assistant/adr/0002-rest-chat-stream-not-ag-ui]]).

## Verification
`cargo clippy -p pando-client --all-targets -- -D warnings`, `cargo test -p pando-client` (unit + fixture replay + mock http/https; run repeatedly, stable), `cargo xtask check-deps`, `cargo deny check licenses`. Live `#[ignore]` tests (`cargo test -p pando-client --test live -- --ignored`) passed against a real `pando serve` (https + real CA, token fetched on first 401, ConnectionService connects).
