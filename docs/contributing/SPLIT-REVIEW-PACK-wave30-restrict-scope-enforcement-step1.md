# SPLIT REVIEW PACK — Wave30 Restrict Scope Enforcement Step1

## Intent
Freeze the bounded runtime enforcement contract for `restrict_scope` before implementation.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add rewrite/filter/redaction runtime paths
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave30-restrict-scope-enforcement.md`
- `docs/contributing/SPLIT-CHECKLIST-wave30-restrict-scope-enforcement-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave30-restrict-scope-enforcement-step1.md`
- `scripts/ci/review-wave30-restrict-scope-enforcement-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. `restrict_scope` validity checks are explicit.
3. Mismatch/missing/unsupported handling is explicit.
4. Scope evidence requirements are explicit.
5. Runtime paths are untouched.
6. No rewrite/filter/redaction scope expansion appears.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave30-restrict-scope-enforcement-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- enforcement contract is frozen cleanly
- Step2 can implement bounded enforcement without reopening semantics
