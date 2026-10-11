---
id: PDA-EP-0015
type: epic
title: Upstream improvements to the Pando API
status: backlog
priority: high
milestone: PDA-M-0004
author: mcp
labels: [upstream-pando]
created: 2026-10-11T21:45:37Z
updated: 2026-10-11T22:02:43Z
---

## Description
Changes in the Pando repo (`/www/MCP/Pando/pando`) that the assistant needs or that greatly improve its robustness: gaps G1–G16 and bug G17 in `docs/research/pando-api.md` §6.2. Each task is implemented in Pando; the assistant degrades gracefully until they exist.

## Acceptance Criteria
- Each change has tests in Pando, is backward compatible (additive fields) and is detectable by the client (capabilities or probe).

## Notes
Priority: G1, G2, G3, G5, G6, G14 (these directly affect the organism and voice).
