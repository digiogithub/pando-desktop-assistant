---
id: PDA-T-0060
type: task
title: Create agent from the UI
status: done
parent: PDA-US-0015
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [shell]
estimate: 2
created: 2026-10-11T21:50:04Z
updated: 2026-10-12T07:53:44Z
started: 2026-10-12T07:53:19Z
closed: 2026-10-12T07:53:44Z
---

## Description
"Nuevo agente" dialog: prompt, engine (list of known engines), optional model, project/work_dir, tags; `POST /orchestrator/tasks` (correct body with `prompt`; the WebUI dialog has the bug of sending `name/description`).

## Acceptance Criteria
- The cell is born with mitosis on creation.
