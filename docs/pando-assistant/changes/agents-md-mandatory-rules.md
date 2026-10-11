---
created_at: 2026-10-11T22:07:40.300738174Z
updated_at: 2026-10-11T22:07:40.300738174Z
tags:
    - change
    - docs
    - agents-md
---
# AGENTS.md: merged MANDATORY operating rules for AI agents

**What changed**
- Added a new `## 0. MANDATORY operating rules for AI agents` section to `AGENTS.md`, merged from the canonical checklist and adapted to this project rather than pasted:
  - 0.1 context first (KB without tags → wiki-link graph via `kb_related_documents` → gintrack task/spec reading → code index incl. `pando`/`bitacora` projects → `recall` only for known keys).
  - 0.2 external research (Context7 for gpui-kit/rust-i18n/sherpa-onnx/tokio…, web search + fetch for facts/licences, browser tools for `organism.html` / JS golden harness / Pando WebUI).
  - 0.3 plan before non-trivial work, saved at `pando-assistant/plans/<slug>_plan.md`, linked to PDA ids.
  - 0.4 small verified increments using the project's cargo check/test/clippy/fmt; never claim done without evidence.
  - 0.5 document every change in KB (`pando-assistant/changes|fixes|features/<slug>.md`) with what/files/why/verification and `[[wiki links]]`; plan-mode note.
  - 0.6 memory tool choice: remember/recall vs KB vs gintrack (never mirror the backlog, never edit `docs/.pmngr/`).
  - 0.7 general conduct: English, subagents per §3, confirm irreversible/outward actions (force push, worktree deletion, cargo clean of others' checkouts, upstream Pando PRs, releases).
- Labelled existing binding sections MANDATORY: §1 Language, §2 Task lifecycle, §4 Worktrees and disk space, §6 Tests.
- Noted that `CLAUDE.md` only imports `AGENTS.md` (`@AGENTS.md`), so it is the single source.

**Evaluation**: existing doc already covered English (kept), subagent briefing (kept, referenced), tests-per-task (kept, labelled). Missing: KB context recovery, research tools, KB plans, KB change docs, memory-tool choice, verification-before-done, confirmation of irreversible actions — all added.

**Files touched**: `AGENTS.md` only (CLAUDE.md unchanged).

**Why**: ensure AI agents follow a clearly labelled MANDATORY rule set integrated with the project's gintrack/worktree/cargo conventions.

**Verification**: re-read the file; headings 0–8 present in order, code fences balanced, all original project sections intact (`git diff --stat`: +123/−4, the 4 being heading relabels and the header line).

Related: [[PDA-US-0005]] (up-to-date docs and decisions for implementers), [[pando-assistant/plans]]
