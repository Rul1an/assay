# SPLIT REVIEW PACK - Wave35 Fulfillment Normalization Step2

## Intent
Implement bounded, additive normalization for obligation fulfillment evidence.

This slice must:
- preserve current execution behavior
- normalize fulfillment evidence deterministically
- keep event payload changes additive

This slice must not:
- add new obligation types
- expand policy-engine scope
- add UI/control-plane/auth transport work
- touch workflow files

## Reviewer focus
1. Diff stays inside bounded runtime/test/docs/gate scope.
2. Normalized outcome fields are deterministic (`reason_code`, `enforcement_stage`, `normalization_version`).
3. Fulfillment path separation is explicit (`policy_deny` vs `fail_closed_deny`).
4. Existing obligation paths remain intact.
5. No scope creep into non-goals.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave35-fulfillment-normalization-step2.sh
```
