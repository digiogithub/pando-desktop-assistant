---
id: PDA-T-0013
type: task
title: Build and smoke test the app in CI (xvfb + lavapipe)
status: backlog
parent: PDA-US-0004
milestone: PDA-M-0001
author: mcp
labels: [ci]
estimate: 3
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:02:09Z
---

## Description
Job test-app: build release and run `--smoke-test` on ubuntu-24.04 (xvfb-run, `VK_ICD_FILENAMES` lavapipe, no `WAYLAND_DISPLAY`), macOS and Windows.

## Acceptance Criteria
- Smoke test green on macOS and Windows; Linux `continue-on-error` documented.
