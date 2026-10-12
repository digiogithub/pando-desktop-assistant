---
created_at: 2026-10-12T11:15:23.695690938Z
updated_at: 2026-10-12T11:15:23.695690938Z
tags:
    - fix
    - decide
    - policy
    - proptest
    - PDA-T-0100
---
# Fix: decide policy monotonicity property (PDA-T-0100)

Builds on [[pando-assistant/plans/backlog-execution_plan.md]] and the trust policy spec [[PDA-SP-0008]] (R2, the confidence policy).

## What changed
`cargo test -p decide` was failing in `policy::tests::monotonic_in_confidence`. The bug was in the property, not in the policy. The shrunk counterexample had action `p = 1.5`. That value is invalid, so `p_of` fail-closes it to 0 and the outcome is Conversation/LowConfidence. The test "lowered" it by multiplying the raw value by k = 0.55, giving 0.83. That is a valid probability, so the decision (a reversible Status command to All agents, minimum p 0.61 ≥ 0.6) executed. Scaling an untrusted input into the valid range is not a lowering of confidence.

The property now lowers the **effective** confidence (`p_of(..) * k`) of every question (intent, action, agent, engine, project), each independently: either unchanged or multiplied by k in [0, 1). It asserts that a lowered decision that executes implies the original decision also executes. This version is stronger than the old one. `policy()` is unchanged, and no fail-closed guarantee was relaxed.

## Files & symbols
- `crates/decide/src/policy.rs#tests::monotonic_in_confidence` (property rewritten, with an explanatory doc comment)
- `crates/decide/proptest-regressions/policy.txt` (the regression seed, now committed)

## Why
Test failure on main. PDA-T-0100 / PDA-SP-0008.R2 (fail-closed confidence policy).

## Verification
- `cargo test -p decide` passed 3 times (70 unit tests).
- The policy proptests passed a temporary 50 000-case run. Note that `with_cases(4000)` overrides `PROPTEST_CASES`.
- `cargo clippy -p decide --all-targets -D warnings` and `cargo fmt` are clean.
- Commit f6e4866.
