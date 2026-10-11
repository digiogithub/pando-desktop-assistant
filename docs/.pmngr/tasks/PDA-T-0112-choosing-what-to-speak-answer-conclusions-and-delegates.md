---
id: PDA-T-0112
type: task
title: "Choosing what to speak: answer, conclusions and delegates"
status: backlog
parent: PDA-US-0032
milestone: PDA-M-0007
author: mcp
labels: [voice-out, pando-client]
estimate: 3
created: 2026-10-11T21:54:29Z
updated: 2026-10-11T22:03:24Z
---

## Description
Final answer = accumulated `content_delta` until `done` of the voice-initiated run; subagent conclusions from `[delegated-result]` in history (or G3 payload) → `summary` + `follow_up`; Claude Stop hook if "avísame" was requested; never tool output; short summary if the answer is long (first paragraph + "tienes el resto en pantalla").

## Acceptance Criteria
- Scenarios of PDA-SP-0005.R6.
