# SPLIT REVIEW PACK - Wave34 Fail-Closed Step1

## Intent
Freeze the bounded fail-closed matrix typing contract before implementation.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add control-plane workflow semantics
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave34-fail-closed-matrix-typing.md`
- `docs/contributing/SPLIT-CHECKLIST-wave34-fail-closed-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave34-fail-closed-step1.md`
- `scripts/ci/review-wave34-fail-closed-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Fail-closed matrix dimensions are explicit.
3. Risk class, fallback mode, and trigger baselines are explicit.
4. Deterministic reason-code baseline is explicit.
5. Runtime paths are untouched.
6. No workflow/control-plane scope expansion appears in this wave.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave34-fail-closed-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- fail-closed contract is frozen cleanly
- Step2 can implement matrix typing without reopening semantics
