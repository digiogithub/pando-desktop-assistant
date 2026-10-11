---
created_at: 2026-10-11T22:13:16.587855953Z
updated_at: 2026-10-11T22:13:16.587855953Z
tags:
    - plan
    - orchestration
---
# Backlog execution plan (orchestrated)

Director: main Claude session. Workers: Claude Code subagents (not mesnada).
- **Opus** → reasoning tasks (design docs, ADRs, spikes, architecture, safety policy, decide).
- **Sonnet** → pure programming tasks (crates, UI, client).
- **Haiku** → simple tasks (locale files es/en, gintrack status/trace updates, comments).

Rules: see [[AGENTS.md]]. Work lands directly on `main` (small tasks, one agent per crate), commits per task
(`Refs: PDA-T-0NNN`), director pushes regularly to `origin/main`. **No GitHub Actions / remote builds for now**
(user decision 2026-10-12): CI tasks PDA-T-0012/13/14, 0091 (CI part), 0147, 0152 deferred.

## Phases (each independently verifiable)
1. **Wave 1 — M0 foundations**: T-0001..0004 (workspace, skeletons, xtask check-deps, deny.toml) sequential by one Sonnet agent; T-0015..0017 docs by Opus in parallel.
2. **Wave 2 — M0 shell + tokens**: T-0005..0011 (ui facade, tokio bridge, fonts, startup, window, tokens xtask).
3. **Wave 3 — M0 spikes**: T-0018..0028 desk-research + code spikes where feasible locally; reports in `docs/spikes/`.
4. **M1 organism** (EP-0003): T-0029..0043.
5. **M2 pando-client + cells** (EP-0004/0005): T-0044..0062.
6. **M3 chat + HITL** (EP-0006/0007).
7. **M4 voice in**, **M5 decide**, **M6 voice out**, **M7 shell**, **M8 claude-bridge**, **M9 wake word**, **M10 release** (packaging items needing certificates/signing deferred).
8. **Upstream Pando** (EP-0015): needs user confirmation before PRs to `/www/MCP/Pando/pando`.

## Status
- 2026-10-12: plan created; wave 1 launched.
