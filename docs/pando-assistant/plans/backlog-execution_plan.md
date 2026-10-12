---
created_at: 2026-10-11T22:13:16.619793246Z
updated_at: 2026-10-12T09:01:50.448479926Z
tags:
    - plan
    - orchestration
---
# Backlog execution plan (orchestrated)

Director: main Claude session. Workers: Claude Code subagents (not mesnada).
- **Opus** → reasoning tasks (design docs, ADRs, spikes, decide/policy, research).
- **Sonnet** → pure programming tasks (crates, UI, client).
- **Haiku** → simple tasks (gintrack status/comment batches, ADR status edits).

Rules: see [[AGENTS.md]]. Common worker brief kept by the director. Work landed on `main` (pure crates) or in
≤ 2 worktrees for parallel app work (`epic/PDA-EP-0006-chat`, `epic/PDA-EP-0008-voice-app`, both merged and removed).
**No GitHub Actions / remote builds for now** (user decision 2026-10-12): CI tasks PDA-T-0012/13/14, 0091, 0147, 0152 deferred.

## Product-owner decisions (2026-10-12)
- Upstream Pando: only indispensable, generic changes; local branch, no push/PR until review. Gap review
  [[pando-assistant/research/upstream-gap-review.md]]: only G2 required (PDA-T-0156, branch
  `task/PDA-T-0156-task-response-fields`, worktree `/www/MCP/Pando/pando-wt/pda-t-0156`, in_review). 13 gap tasks cancelled; G4/G5 low priority.
- ADR 0007 accepted (pando-stt permissive / pando-tts GPL-3 split). ADR 0008 accepted (Whistle default STT).
- T-0017 design defaults approved (colours, 12-cell cap, no idle/listening ripples).
- Hardware/credential-dependent tasks: implement what's possible, leave in_review with pending list.

## Status (2026-10-12, end of first orchestration run)
- M0–M10 implemented on main; workspace green: fmt, clippy -D warnings, all tests, check-deps, tokens --check, cargo deny licenses, xvfb smoke + demo.
- Remaining open: CI (deferred), PDA-T-0154 beta QA (humans), PDA-T-0135 (upstream v2), G4/G5 (optional upstream), PDA-T-0171 (voice app-level owner; in progress), many tasks in_review pending real hardware / other OS / live Pando / design sign-off.
- Local machine needs `libasound2-dev libxkbcommon-x11-dev libpipewire-0.3-dev` (see script/install-linux-deps.sh).
