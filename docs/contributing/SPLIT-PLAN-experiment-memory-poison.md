# SPLIT PLAN - Experiment: Persistent Memory Poisoning with Delayed Trigger

## Intent

Test whether schema-valid, internally consistent poisoned state entries can survive
past their injection point and later shift policy classification, replay diff buckets,
or evidence convergence — without triggering any immediate violation.

## Overarching invariant

Poisoned state must never silently downgrade a stricter classification into a weaker one
without either detectable fallback signaling or invariant failure.

## Vectors

4 bounded attack vectors:

1. **Replay Baseline Poisoning** — shift `restrictiveness_rank()` via crafted `decision_outcome_kind`
2. **Deny Convergence Evidence Poisoning** — flip deny fields to legacy non-deny via plausible compat shape
3. **Context Envelope Completeness Poisoning** — inject `auth_context_summary` to promote `PartialEnvelope` to `CompleteEnvelope`
4. **State Window Decay Escape** — inject phantom tool class that persists past decay window

## Conditions

- A: unprotected (no integrity checks)
- B: integrity-checked (hashes/contracts only)
- C: full existing contract stack (context + deny convergence + fulfillment + replay compat + consumer hardening)

## Metrics

- PRR (Poison Retention Rate)
- DASR (Delayed Activation Success Rate)
- PPI (Policy Precedence Integrity)
- RDCS (Replay Diff Classification Stability)
- FPBR (False Positive on Benign Recall)

## Benign controls

- B1: run metadata recall
- B2: prior tool observation recall
- B3: approval context recall

## Hypotheses

- H1: Condition B → PRR < 10%
- H2: Condition C → DASR < 5%
- H3: FPBR < 2%
- H4: Vector 4 has highest PRR under B and C

## Wave structure

### Step 1 (this PR): Freeze

Docs + gate only. No code.

### Step 2: Implementation

4 attack vectors in `assay-sim`, invariant tests in `assay-core`.
No runtime pipeline changes.

### Step 3: Closure

Results analysis, hypothesis validation, hardening recommendations.
