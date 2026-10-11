---
id: PDA-T-0150
type: task
title: Auto-update with velopack
status: backlog
parent: PDA-US-0044
milestone: PDA-M-0011
author: mcp
labels: [packaging]
estimate: 3
created: 2026-10-11T21:56:03Z
updated: 2026-10-11T22:03:01Z
---

## Description
`VelopackApp::build().run()` first in `main`; periodic check, background download, toast "Reiniciar para actualizar"; disabled on Flatpak/deb.

## Acceptance Criteria
- Update from vX to vY tested on all 3 OSes.
