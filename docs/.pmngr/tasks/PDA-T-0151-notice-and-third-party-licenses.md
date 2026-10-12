---
id: PDA-T-0151
type: task
title: NOTICE and third-party licenses
status: in_progress
parent: PDA-US-0045
milestone: PDA-M-0011
assignees: [claude-sonnet]
author: mcp
labels: [packaging, legal]
estimate: 2
created: 2026-10-11T21:56:03Z
updated: 2026-10-12T08:00:57Z
started: 2026-10-12T08:00:57Z
---

## Description
Generate the Rust license inventory (`cargo-about` or similar) + manual entries for needle/Whistle, ONNX Runtime, voices, fonts and the worker's espeak-ng (GPL source offer); "About" view with licenses.

## Acceptance Criteria
- `cargo deny` shows no GPL in the `app` graph; NOTICE included in all artifacts.
