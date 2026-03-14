# SPLIT REVIEW PACK - Wave40 Deny Evidence Step1

## Intent
Freeze the bounded deny evidence convergence contract before any Step2 implementation.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add new obligation/runtime capability
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave40-deny-evidence-convergence.md`
- `docs/contributing/SPLIT-CHECKLIST-wave40-deny-evidence-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave40-deny-evidence-step1.md`
- `scripts/ci/review-wave40-deny-evidence-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Deny separation markers are explicit (`policy_deny`, `fail_closed_deny`, `enforcement_deny`).
3. Deny precedence contract is explicit and deterministic.
4. Legacy deny fallback compatibility is explicit.
5. Runtime paths are untouched.
6. No runtime behavior change is introduced in this wave.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave40-deny-evidence-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- deny convergence contract is frozen cleanly
- Step2 can implement bounded normalization without reopening semantics
