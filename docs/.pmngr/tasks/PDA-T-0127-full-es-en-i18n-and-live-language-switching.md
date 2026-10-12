---
id: PDA-T-0127
type: task
title: Full es/en i18n and live language switching
status: in_review
parent: PDA-US-0038
milestone: PDA-M-0008
assignees: [claude-sonnet]
author: mcp
labels: [shell, i18n]
estimate: 2
created: 2026-10-11T21:54:29Z
updated: 2026-10-12T08:19:06Z
started: 2026-10-12T08:19:06Z
---

## Description
Review that all UI text and all spoken phrases come from locales; system language detection (`sys-locale`); live switching that also affects GPUI Kit; test that fails on untranslated keys.

## Acceptance Criteria
- Zero UI literals in code (lint/grep in CI).
