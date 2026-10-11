---
id: PDA-T-0102
type: task
title: Voice confirmation dialog and top-2
status: backlog
parent: PDA-US-0029
milestone: PDA-M-0006
author: mcp
labels: [decide, voice-out]
estimate: 3
created: 2026-10-11T21:52:55Z
updated: 2026-10-11T22:03:24Z
---

## Description
Spoken confirmations ("¿Cancelo migracion-auth?") answered yes/no via PTT (question `noul`) or buttons; low confidence ⇒ two buttons with the best options; timeout ⇒ do not execute.

## Acceptance Criteria
- With no answer within 8 s, nothing is executed.
