---
created_at: 2026-10-12T14:13:42.388450423Z
updated_at: 2026-10-12T14:13:42.388450423Z
tags:
    - pando-assistant
    - change
    - decide
    - safety
    - adr
---
# decide: open questions settled (ADR 0009) + "háblale a" / "vuelve" (PDA-T-0172)

Builds on [[pando-assistant/changes/decide-decision-layer.md]] and [[pando-assistant/fixes/decide-policy-monotonicity.md]]; serves [[pando-assistant/plans/backlog-execution_plan.md]]. Spec PDA-SP-0008 (R2, new R8). ADR: `docs/adr/0009-decision-confidence-min-and-explicit-only-approvals.md`.

## What changed
Product-owner decisions of 2026-10-12:
1. **Command confidence = minimum** across the cascade questions (kept). Doc comments in `crates/decide/src/policy.rs` (module header and where `p` is computed in `policy`), ADR 0009, comment on PDA-T-0100.
2. **Flagged approvals only by click**; a spoken "sí" never approves (kept). ADR 0009; new end-to-end test `crates/app/src/voice/e2e.rs#a_flagged_approval_is_never_approved_by_a_spoken_yes` (policy → ExplicitOnly; stale snapshot → Confirm → "sí" → executor → real `pando_client::approvals::Approvals` refuses `Origin::Voice`; read-aloud "sí" → NeedsScreen). Comment on PDA-T-0103.
3. **New PDA-SP-0008.R8 / task PDA-T-0172**: "háblale a <agente>" and "vuelve" as reversible target switches.

## Files & symbols
- `crates/decide/src/questions.rs`: `TargetTrigger`, `target_trigger` (rule lead-ins: háblale a/al, habla con, hablar con, quiero hablar con, pásame con, ponme con; whole-utterance vuelve/volver/vuelve a Pando/… with fillers).
- `crates/decide/src/cascade.rs`: `TALK_TO_AGENT_CLASS`, `BACK_TO_PANDO_CLASS`, `Step::Talk` (agent question only), rule decisions in `Cascade::new`.
- `crates/decide/src/policy.rs`: `VoiceTarget`, `Outcome::SwitchTarget`, `talk_to`; proptests `switch_target_is_safe`, `monotonic_in_confidence` extended to switches.
- `crates/decide/tests/live_intent.rs` + `fixtures/target_phrases.tsv`.
- `crates/app/src/cells/pane.rs#OrganismPane::voice_switch_target`, `voice/assistant.rs` (SwitchTarget arm), `voice/session.rs` (`Decided::SwitchTarget`), `decision/engine.rs`, locales `voice.target.{talking,back}`.
- Docs: ADR 0009 + index, `docs/03-decision-layer.md`.

## Why rules, not intent classes
Measured with Ollama 0.35 + tev1:0.8b: adding `talk_to_agent`/`back_to_pando` classes dropped intent accuracy 96/100 → 88/100 (87 with narrower descriptions); "volver" went to rerun_agent. With rules: 96/100 (same 4 misses as baseline), near misses keep their class 3/3, no labelled phrase triggers the rule.

## Verification
cargo fmt; clippy -p decide / -p app --all-targets -D warnings clean; cargo test -p decide (75 lib, proptests 4000 cases), cargo test -p app 352 passed (xvfb); cargo xtask check-deps OK; live evals as above. Commits 0497234, 6c2610f.
