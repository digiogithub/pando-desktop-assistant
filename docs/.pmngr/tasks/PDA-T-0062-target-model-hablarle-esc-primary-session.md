---
id: PDA-T-0062
type: task
title: "Target model: Hablarle / Esc / primary session"
status: in_review
parent: PDA-US-0016
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [shell, pando-client]
estimate: 2
created: 2026-10-11T21:50:04Z
updated: 2026-10-12T07:53:44Z
started: 2026-10-12T07:53:19Z
---

## Description
State `VoiceTarget { Mesnada(session_id) | Agent{task_id, session_id} }`; primary session = the one chosen by the user or the most recent one in the active project; "Hablarle" only for talkable engines with a known session; Esc returns to mesnada.

## Acceptance Criteria
- Transition tests; the action is disabled with a tooltip if `acp_session_id` is missing (until G2).
