---
id: PDA-T-0147
type: task
title: Per-OS install and launch smoke tests in CI
status: backlog
parent: PDA-US-0043
milestone: PDA-M-0011
author: mcp
labels: [packaging, ci]
estimate: 2
created: 2026-10-11T21:56:03Z
updated: 2026-10-11T22:03:01Z
---

## Description
Workflow `bundle.yml`: build each artifact, install it on a clean runner, run `--smoke-test`, verify that `pando-voice` responds to `Health`.

## Acceptance Criteria
- Green matrix before each release.
