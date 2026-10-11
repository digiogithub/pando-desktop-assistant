---
id: PDA-T-0015
type: task
title: AGENTS.md / CLAUDE.md and initial ADR log
status: in_progress
parent: PDA-US-0005
milestone: PDA-M-0001
assignees: [claude-opus]
author: mcp
labels: [docs]
estimate: 2
created: 2026-10-11T21:47:41Z
updated: 2026-10-11T22:13:55Z
started: 2026-10-11T22:13:55Z
---

## Description
Repo rules (crates, deps, tokio bridge, tests, i18n, language), and ADRs: 001 gpui-kit pin + facade; 002 REST `chat/stream` protocol (not AG-UI); 003 rust-i18n; 004 `pando-voice` worker in a separate process (Windows ABI, crash isolation, GPL); 005 sherpa-onnx as the only ORT; 006 tiny-skia only for tests/fallback.

## Acceptance Criteria
- ADRs linked from `docs/README.md`.
