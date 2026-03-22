# Wave G1 Step 1 Review Pack

## Reviewer Questions

1. Does `G1` stay strictly on weaker-than-requested containment while execution continued?
2. Are intentional audit/permissive runs and fail-closed aborts clearly excluded?
3. Does the payload stay typed, small, and non-authoritative in `detail`?
4. Is cardinality stable, with duplicate degradation observations suppressed?
5. Does the repo truth now support saying `A5-002` is no longer a pure signal gap without overclaiming sandbox correctness?

## Expected Outcome

- supported sandbox fallback paths can emit `assay.sandbox.degraded`
- healthy runs and fail-closed aborts do not emit that event
- `A5-002` remains outside the shipped subset, but it is no longer blocked by total signal absence
