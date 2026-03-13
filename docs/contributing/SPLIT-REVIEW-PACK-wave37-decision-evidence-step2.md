# SPLIT REVIEW PACK - Wave37 Decision Evidence Convergence Step2

## Intent
Implement bounded, additive decision/evidence convergence normalization.

This slice must:
- keep existing execution behavior unchanged
- add deterministic convergence classification fields
- keep event payload changes additive and backward-compatible

This slice must not:
- add new obligation types
- add new runtime capability
- expand policy-engine scope
- add UI/control-plane/auth transport work
- touch workflow files

## Reviewer focus
1. Diff stays inside bounded runtime/test/docs/gate scope.
2. Convergence fields are present and deterministic.
3. Policy deny vs fail-closed deny remains explicitly separated.
4. Existing obligation execution behavior remains intact.
5. No scope creep into non-goals.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave37-decision-evidence-step2.sh
```
