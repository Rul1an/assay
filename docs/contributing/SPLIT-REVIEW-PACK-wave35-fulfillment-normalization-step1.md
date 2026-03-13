# SPLIT REVIEW PACK — Wave35 Fulfillment Normalization Step1

## Intent
Freeze the Wave35 fulfillment normalization contract before any implementation.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add new obligation types
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave35-obligation-fulfillment-normalization.md`
- `docs/contributing/SPLIT-CHECKLIST-wave35-fulfillment-normalization-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave35-fulfillment-normalization-step1.md`
- `scripts/ci/review-wave35-fulfillment-normalization-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Normalized `obligation_outcomes` shape is explicit.
3. Deterministic `reason_code`, `enforcement_stage`, `normalization_version` requirements are explicit.
4. Separation model is explicit (`policy_deny` vs `fail_closed_deny` vs obligation statuses).
5. Runtime paths are untouched.
6. No new obligation type is introduced in this wave.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave35-fulfillment-normalization-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- normalization contract is frozen cleanly
- Step2 can implement additive normalization without reopening semantics
