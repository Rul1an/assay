# SPLIT REVIEW PACK - Wave33 Obligation Outcomes Step1

## Intent
Freeze the bounded normalization contract for `obligation_outcomes` before implementation.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add new obligation execution semantics
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave33-obligation-outcomes-normalization.md`
- `docs/contributing/SPLIT-CHECKLIST-wave33-obligation-outcomes-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave33-obligation-outcomes-step1.md`
- `scripts/ci/review-wave33-obligation-outcomes-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Normalized outcome fields are explicit.
3. Canonical status semantics are explicit.
4. Reason-code baseline is explicit.
5. Runtime paths are untouched.
6. No behavior scope expansion beyond additive normalization contract.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave33-obligation-outcomes-step1.sh
```

## Expected outcome
- gate passes
- runtime code is untouched
- obligation outcome normalization contract is frozen cleanly
- Step2 can implement additive normalization without reopening semantics
