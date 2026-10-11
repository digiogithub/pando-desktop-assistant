---
id: PDA-US-0004
type: story
title: As a team we want multiplatform CI from the first commit
status: backlog
priority: high
parent: PDA-EP-0001
milestone: PDA-M-0001
author: mcp
labels: [infra, ci]
created: 2026-10-11T21:46:17Z
updated: 2026-10-11T22:02:01Z
---

## Description
GitHub Actions: checks (fmt, typos, machete, cargo-deny, check-deps, tokens --check), test-core on ubuntu/macos/windows, clippy on macOS, build + app smoke test (xvfb + lavapipe on Linux), weekly GPUI Kit canary.

## Acceptance Criteria
- PR with a clippy or deny failure is blocked.
- App smoke test runs on the 3 OSes (Linux `continue-on-error` until stabilized).

## Notes
Refs: `docs/research/bitacora-gpui.md` §4 (workflows `ci.yml`, `gpui-kit-canary.yml`).
