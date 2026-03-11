# SPLIT REVIEW PACK — Wave27 Approval Step1

## Intent
Freeze the approval artifact contract before any runtime enforcement of `approval_required`.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add approval enforcement
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave27-approval-artifact.md`
- `docs/contributing/SPLIT-CHECKLIST-wave27-approval-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave27-approval-step1.md`
- `scripts/ci/review-wave27-approval-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Approval artifact minimum fields are explicit.
3. Freshness / expiry semantics are explicit.
4. Tool/resource binding is explicit.
5. Additive approval evidence fields are explicit.
6. Runtime paths are untouched.
7. No approval enforcement is introduced in this wave.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave27-approval-step1.sh
```

## Expected outcome
- gate passes
- runtime code is untouched
- approval contract is frozen cleanly
- Step2 can implement artifact/evidence shape without reopening semantics
