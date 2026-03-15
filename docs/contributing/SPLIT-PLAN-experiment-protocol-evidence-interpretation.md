# SPLIT PLAN - Experiment: Protocol Evidence Interpretation Attacks

## Intent

Test consumer-side trust downgrade under partial, ambiguous, or flattened protocol
evidence. When does protocol-valid but incompletely interpreted metadata lead to
an overly optimistic trust decision?

## Overarching Invariant

> A consumer must never produce a weaker classification than the canonical
> full-contract interpretation, regardless of which field subset it reads.

## Vectors

1. **Partial-Field Trust Read** — consumer reads legacy `decision`, ignores converged `decision_outcome_kind` (consumer-realistic synthetic)
2. **Precedence Inversion** — consumer reads correct deny fields in wrong tier order (producer-realistic)
3. **Compat Flattening** — consumer suppresses compat/trust signals, treats CompatibilityFallback as Converged (consumer-realistic)
4. **Projection Loss** — consumer drops required fields in transit, falls to weaker read path (adapter-realistic)

## Trust Signal Classification

- Verified: backed by observed emission / provenance / identity
- Self-reported: declared without independent verification
- Inferred: derived from partial signals or absence of markers

## Conditions

- A: unprotected (reads legacy `decision` only)
- B: precedence-aware, trust-incomplete (follows read-path, treats compat as non-binding)
- C: full consumer hardening (compat binding + required-field completeness + tier precedence)

## Metrics

- CDR, PIR, CFR, PLR (per-vector)
- CCAR (global canonical agreement)
- FPBR (false positive on benign)

## Wave structure

- Step 1 (this PR): Freeze — docs + gate only
- Step 2: Implementation — 4 vectors, 3 controls, A/B/C matrix
- Step 3: Closure — results, hypothesis validation, trifecta synthesis
