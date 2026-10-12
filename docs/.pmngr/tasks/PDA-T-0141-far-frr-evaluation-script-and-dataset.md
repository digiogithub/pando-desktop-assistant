---
id: PDA-T-0141
type: task
title: FAR/FRR evaluation script and dataset
status: in_review
parent: PDA-US-0042
milestone: PDA-M-0010
assignees: [claude-sonnet]
author: mcp
labels: [wake-word, testing]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-12T08:13:08Z
started: 2026-10-12T08:12:36Z
---

## Description
Dataset (3 h of office/podcast/TV noise + positives at 0.5/1/2 m from several voices), reproducible script that runs the detector offline and reports FAR/hour, FRR and CPU; runs in nightly CI on a subset.

## Acceptance Criteria
- Thresholds from PDA-SP-0011.R3 verified and documented.
