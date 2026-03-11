# SPLIT REVIEW PACK — Wave29 Restrict Scope Step1

## Intent
Freeze the `restrict_scope` contract before any runtime enforcement/execution semantics are introduced.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add `restrict_scope` runtime enforcement
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave29-restrict-scope.md`
- `docs/contributing/SPLIT-CHECKLIST-wave29-restrict-scope-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave29-restrict-scope-step1.md`
- `scripts/ci/review-wave29-restrict-scope-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Restrict-scope contract fields are explicit.
3. Scope mismatch reasons are explicit.
4. Additive scope evidence fields are explicit.
5. Runtime paths are untouched.
6. No `restrict_scope` runtime execution is introduced in this wave.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave29-restrict-scope-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- restrict-scope contract is frozen cleanly
- Step2 can implement contract representation without reopening semantics
