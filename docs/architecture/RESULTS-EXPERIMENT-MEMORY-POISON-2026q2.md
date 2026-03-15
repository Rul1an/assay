# Experiment Results: Persistent Memory Poisoning with Delayed Trigger (Q2 2026)

## Status

Experiment complete. All hypotheses tested. Closure.

## Executive Summary

The full existing Assay contract stack stops 100% of tested delayed memory/context
poisoning vectors while producing zero false positives on benign controls.

Integrity checks alone (content-addressed hashes) reduce the attack surface by 75%.
The remaining 25% — schema-valid context completeness poisoning — requires field
provenance validation, which the full contract stack provides.

## Results Matrix

### Per-Condition

| Metric | Condition A (unprotected) | Condition B (integrity) | Condition C (full stack) |
|--------|--------------------------|------------------------|-------------------------|
| DASR | 12/12 (100%) | 3/12 (25%) | 0/12 (0%) |
| Misclassifications | 9 | 0 | 0 |
| Policy shifts | 3 | 3 | 0 |
| Correctly detected | 0 | 9 | 12 |
| FPBR | 0% | 0% | 0% |

### Per-Vector

| Vector | Target | Cond A | Cond B | Cond C |
|--------|--------|--------|--------|--------|
| V1 Replay Baseline | `classify_replay_diff()` | Misclassification | Detected | Detected |
| V2 Deny Convergence | Deny precedence fields | Misclassification | Detected | Detected |
| V3 Context Envelope | `project_context_contract()` | Policy Shift | Policy Shift | Detected |
| V4 Decay Escape | ADR-029 decay window | Misclassification | Detected | Detected |

### Defense Layer Contribution

| Transition | Vectors blocked | Mechanism |
|-----------|----------------|-----------|
| A → B | V1, V2, V4 | Content-addressed hash comparison on replay baselines and state snapshots |
| B → C | V3 | Field provenance validation via context contract |

## Hypothesis Validation

| ID | Statement | Result | Evidence |
|----|-----------|--------|----------|
| H1 | PRR < 10% under Condition B | **Confirmed** | PRR = 0% for V1/V2/V4 |
| H2 | DASR < 5% under Condition C | **Confirmed** | DASR = 0% |
| H3 | FPBR < 2% | **Confirmed** | FPBR = 0.0% |
| H4 | V4 has highest PRR under B/C | **Partially refuted** | V3, not V4, is the hardest vector |

## Key Findings

### 1. Integrity checks are necessary but not sufficient

Content-addressed hashes block 75% of vectors (V1/V2/V4). But V3 survives because
the injected `auth_context_summary` is schema-valid — it passes hash comparison since
it was never in the original baseline to compare against.

### 2. Field provenance validation is the decisive layer

V3 is only blocked under Condition C, where the context contract validates that fields
were present at original decision time. This field provenance check catches injections
that are schema-valid but contextually false.

### 3. V3 (context envelope) is the hardest vector, not V4 (decay escape)

This was not predicted by H4. The decay window's snapshot-level hash is sufficient to
detect V4, but the context contract's field-level completeness check has a structural
gap that only provenance validation closes.

### 4. Zero false positives across all conditions

FPBR = 0% means the defense mechanisms do not harm legitimate delayed-memory recall.
Controls B1 (run metadata), B2 (tool observation), B3 (approval context) all pass cleanly.

## Design Implications

### Must preserve

- **Field provenance validation** in context contract as a first-class security invariant
- **Content-addressed hashing** on replay baselines and state snapshots
- **Deny convergence field consistency** checks

### Must not bypass

- Future consumer or replay changes must not skip provenance validation
- Broader context fields (beyond the current 4 required fields) should follow
  the same provenance discipline

### Hardening recommendation

No immediate runtime changes needed — the existing stack handles all tested vectors.
The recommendation is to:
1. Document field provenance validation as an explicit security invariant
2. Add provenance validation to future context field expansions
3. Monitor for V3-class attacks in any future context enrichment features

## Experiment Infrastructure

- **Harness:** `crates/assay-sim/src/attacks/memory_poison.rs` (~650 LOC)
- **Tests:** 15 (9 unit + 6 integration)
- **Matrix:** 45 runs (4 vectors * 3 conditions * 3 delays + 3 controls * 3 delays)
- **All deterministic, no LLM calls**

## References

- Frozen contract: [PLAN-EXPERIMENT-MEMORY-POISON-DELAYED-TRIGGER-2026q2.md](PLAN-EXPERIMENT-MEMORY-POISON-DELAYED-TRIGGER-2026q2.md)
- Wave plan: [SPLIT-PLAN-experiment-memory-poison.md](../contributing/SPLIT-PLAN-experiment-memory-poison.md)
- PR #867 (Step 1 freeze), PR #868 (Step 2 implementation + results)
