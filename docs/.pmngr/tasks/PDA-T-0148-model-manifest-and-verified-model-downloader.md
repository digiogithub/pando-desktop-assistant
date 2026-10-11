---
id: PDA-T-0148
type: task
title: Model manifest and verified model downloader
status: backlog
parent: PDA-US-0044
milestone: PDA-M-0011
author: mcp
labels: [packaging, security]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-11T22:03:01Z
---

## Description
Embedded `models.toml` (id, HF URL pinned to commit SHA, sha256, size, license, attribution); reqwest download to `.part` with resume, SHA-256 verification, atomic rename; `PANDO_ASSISTANT_MODELS_DIR`; `pando-assistant models list|import|verify`.

## Acceptance Criteria
- Wrong hash ⇒ file deleted and clear error.
