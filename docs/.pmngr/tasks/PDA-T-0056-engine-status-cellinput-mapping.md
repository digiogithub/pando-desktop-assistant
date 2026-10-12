---
id: PDA-T-0056
type: task
title: Engine/status → CellInput mapping
status: done
parent: PDA-US-0014
milestone: PDA-M-0003
assignees: [claude-sonnet]
author: mcp
labels: [organism]
estimate: 2
created: 2026-10-11T21:50:04Z
updated: 2026-10-12T07:53:44Z
started: 2026-10-12T07:42:16Z
closed: 2026-10-12T07:53:44Z
---

## Description
Pure function from the table in `docs/06-ui-mapping.md` (corrected: `agent` = engine in `TaskResponse`; label = `tags[0]` or the first 24 chars of `name`), including `cancelled` → Done + 10 s fade and custom engines from `.pando/mesnada/engines/*.template.yaml` by `output_format`.

## Acceptance Criteria
- Full engines × status table test.
