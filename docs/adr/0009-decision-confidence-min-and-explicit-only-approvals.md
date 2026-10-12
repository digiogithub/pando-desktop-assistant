# 0009 · Decision-layer confidence aggregation and explicit-only approvals

- **Status:** accepted (2026-10-12)
- **Date:** 2026-10-12
- **Deciders:** project owner (Jose F. Rives Lirola), recorded by claude-opus
- **Refs:** PDA-SP-0008.R2, PDA-SP-0008.R8, PDA-T-0100, PDA-T-0103, PDA-T-0172, [`docs/03-decision-layer.md`](../03-decision-layer.md)

## Context

The decision layer (`crates/decide`) asks the System One model a cascade of typed questions
(intent → agent + action → engine + project) and the code decides what runs. Three questions
were left open after PDA-T-0100/0103:

1. How to combine the probabilities of the several questions a command depends on into the
   one `p` the policy table uses (minimum, product, mean, …).
2. Whether a spoken "sí" may approve a request flagged `require_explicit_approval` /
   `never_auto_approve` (for example as the answer to a voice confirmation or to a request
   read aloud).
3. Whether to support "háblale a <agente>" / "vuelve" as voice commands.

The implementation already used the minimum and refused flagged approvals by voice; the
property tests (`policy.rs`: `monotonic_in_confidence`, `flagged_approval_never_executes`)
were written against that behaviour.

## Decision

1. **Confidence of a command = the minimum** over every question it depends on (intent;
   agent when the action needs one; action; engine and project for `spawn`; agent for
   "háblale a"). A missing or invalid probability counts as 0. The weakest question also
   supplies the top-2 clarification options.
2. **Flagged approvals are explicit-only: only a click approves them.** The policy returns
   `Outcome::ExplicitOnly` whatever the confidence, and the app's approvals ledger
   (`pando-client` `Approvals`) refuses `Origin::Voice` approvals of flagged items as a
   second, independent barrier — so a spoken "sí" to a voice confirmation or to a request
   read aloud never approves one. Denying by voice stays allowed (it is the safe side).
3. **"háblale a <agente>" and "vuelve" are added** (`PDA-SP-0008.R8`) as non-destructive
   target switches: executed directly at p ≥ the reversible threshold (0.6) without a
   confirmation, clarified with top-2 below it.

## Consequences

- The policy stays monotonic: lowering any answer can never turn a confirmation into an
  execution (proptest `monotonic_in_confidence`, extended to target/project switches).
- The minimum is conservative: a command with several fairly-confident answers (e.g. 0.8 ×
  3) confirms more often than a mean would; this is accepted for safety.
- Flagged approvals always cost a click, even when the user is away from the keyboard and
  speaking; the assistant selects the cell and says "eso requiere confirmación en pantalla".
- Tests: `crates/decide/src/policy.rs#tests::flagged_approvals_are_explicit_only`,
  `#tests::flagged_approval_never_executes`, and the end-to-end
  `crates/app/src/voice/e2e.rs#a_flagged_approval_is_never_approved_by_a_spoken_yes`.
- Revisit if the decision model becomes calibrated enough that a joint probability (product)
  is meaningful, or if Pando adds a server-side "voice may approve" flag.

## Alternatives considered

| Option | Why not |
|---|---|
| Product of probabilities | Not calibrated for independent questions of a 0.8b model; drops below the thresholds too easily and makes the table hard to explain |
| Mean of probabilities | One very weak answer (wrong agent) can be hidden by strong others; breaks fail-closed |
| Spoken "sí" approves flagged items after a confirmation | Defeats the purpose of `require_explicit_approval`; STT/VAD false positives would approve risky actions |
| "háblale a"/"vuelve" as model intent classes | Measured: live intent accuracy fell from 96/100 to 88/100 with `tev1:0.8b`; recognised by rule instead |
