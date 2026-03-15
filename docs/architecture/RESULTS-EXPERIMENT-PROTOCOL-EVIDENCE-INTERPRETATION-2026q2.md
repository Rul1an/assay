# Experiment Results: Protocol Evidence Interpretation Attacks (Q2 2026)

## Status

Experiment complete. All hypotheses tested. Trifecta closure.

## Executive Summary

The full Assay consumer hardening stack (Condition C) produces 100% canonical
consumer agreement (CCAR) with zero silent downgrades and zero false positives.

Condition B (precedence-aware but trust-incomplete) blocks V1 and V3 but not V2
and V4 — precedence awareness alone is insufficient when deny tier ordering or
required-field completeness is not enforced.

## Results Matrix

### Per-Condition

| Metric | Condition A | Condition B | Condition C |
|--------|------------|------------|------------|
| Downgrades | 4/4 (100%) | 2/4 (50%) | 0/4 (0%) |
| **CCAR** | 0% | 50% | **100%** |
| FPBR | 0% | 0% | 0% |

### Per-Vector

| Vector | Realism | Cond A | Cond B | Cond C |
|--------|---------|--------|--------|--------|
| V1 Partial Trust Read | consumer_realistic_synthetic | Silent Downgrade | No Effect | No Effect |
| V2 Precedence Inversion | producer_realistic | Silent Downgrade | Silent Downgrade | No Effect |
| V3 Compat Flattening | consumer_realistic | Silent Downgrade | No Effect | No Effect |
| V4 Projection Loss | adapter_realistic | Silent Downgrade | Silent Downgrade | Detected |

### Defense Layer Contribution

| Transition | Vectors blocked | Mechanism |
|-----------|----------------|-----------|
| A → B | V1, V3 | Read-path precedence (uses converged fields when available) |
| B → C | V2, V4 | Deny tier-1 enforcement + required-field completeness validation |

## Hypothesis Validation

| ID | Statement | Result | Evidence |
|----|-----------|--------|----------|
| H1 | CDR/PIR < 10% under B | **Partially confirmed** | V1 blocked, V2 still bypasses (PIR remains under B) |
| H2 | All rates < 5% under C | **Confirmed** | CCAR = 100%, zero downgrades |
| H3 | FPBR < 2% | **Confirmed** | 0/9 false positives |
| H4 | V3 highest CFR under B | **Refuted** | V3 is blocked by B (compat flattening caught by read-path); V2 and V4 survive B |

## Key Findings

### 1. Read-path precedence blocks partial reads and compat flattening

V1 (partial trust read) and V3 (compat flattening) are both blocked under
Condition B. Once a consumer respects `consumer_read_path` and uses converged
fields, it stops reading legacy `decision` directly and stops treating compat
as converged.

### 2. Deny tier ordering and required-field completeness are the decisive layers

V2 (precedence inversion) survives B because the consumer reads the right deny
fields but in the wrong tier order. Only tier-1 enforcement (`decision_outcome_kind`
wins over legacy deny flags) in Condition C stops it.

V4 (projection loss) survives B because field completeness is not checked — a dropped
`decision_outcome_kind` silently falls to legacy. Only required-field validation in
Condition C catches it.

### 3. The hardest vectors are different from prior experiments

| Experiment | Hardest vector | Blocked by |
|-----------|---------------|-----------|
| Memory Poisoning | V3 (context envelope) | C only (field provenance) |
| Delegation Spoofing | V3 (identity spoofing) | C only (trust-domain) |
| **This experiment** | **V2 + V4** (precedence + projection) | **C only** (tier enforcement + completeness) |

This experiment is the first where the hardest vectors are not a single V3 but two
vectors that require different C-layer mechanisms.

### 4. CCAR progression confirms layered defense

- A: 0% (all consumers downgrade)
- B: 50% (precedence helps but is insufficient)
- C: 100% (full hardening produces canonical agreement)

## Trifecta Synthesis

All three experiments show the same structural pattern with condition-specific nuances:

| Experiment | A (unprotected) | B (integrity/precedence) | C (full stack) | FPBR |
|-----------|----------------|------------------------|---------------|------|
| Memory Poisoning | 100% DASR | 25% DASR | **0% DASR** | 0% |
| Delegation Spoofing | 4/4 bypass | 1/4 bypass | **0/4 bypass** | 0% |
| Protocol Evidence | 0% CCAR | 50% CCAR | **100% CCAR** | 0% |

The overarching lesson across all three:

> **Integrity/precedence (B) handles most attacks, but the hardest attack in each
> experiment requires a deeper verification layer (C):** field provenance for state
> poisoning, trust-domain verification for delegation spoofing, and tier enforcement
> + completeness validation for consumer interpretation.

## Design Implications

### Must preserve

- `consumer_read_path` as mandatory consumer contract (not optional metadata)
- `required_consumer_fields` completeness enforcement
- Deny convergence tier-1 precedence (`decision_outcome_kind` wins)
- `consumer_payload_state` as binding trust signal (compat != converged)

### Hardening recommendation

- Consumer implementations should fail-closed on missing required fields
- Deny classification must follow the 4-tier precedence, not legacy flags
- SDK/analytics chains should preserve all required fields or explicitly
  signal projection loss

## References

- Frozen contract: [PLAN-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md](PLAN-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md)
- PR #871 (Step 1 freeze), this PR (Step 2 implementation + results)
