# SPLIT REVIEW PACK — Wave28 Approval Required Step1

## Intent
Freeze the bounded runtime enforcement contract for `approval_required` before implementation.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add approval UI/case-management
- add external approval services
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave28-approval-required.md`
- `docs/contributing/SPLIT-CHECKLIST-wave28-approval-required-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave28-approval-required-step1.md`
- `scripts/ci/review-wave28-approval-required-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Approval validity checks are explicit.
3. Missing/expired/mismatch behavior is explicit.
4. Additive approval evidence fields are explicit.
5. Runtime paths are untouched.
6. No approval workflow scope expansion is introduced in this wave.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave28-approval-required-step1.sh
```

## Expected outcome
- gate passes
- runtime code is untouched
- approval enforcement contract is frozen cleanly
- Step2 can implement bounded enforcement without reopening semantics
