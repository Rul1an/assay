# SPLIT REVIEW PACK - Wave25 Obligations Step1

## Intent
Freeze Wave25 scope before implementation by defining a bounded obligations execution contract.

This slice is docs + gate only.

It must not:
- change MCP runtime code
- change CLI normalization/runtime behavior
- change MCP server behavior
- change workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave25-obligations-log.md`
- `docs/contributing/SPLIT-CHECKLIST-wave25-obligations-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave25-obligations-step1.md`
- `scripts/ci/review-wave25-obligations-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Execution scope is explicitly frozen to `log`.
3. `legacy_warning` compatibility mapping is explicit.
4. Additive `obligation_outcomes` field is explicit.
5. Non-goals (`approval_required`, `restrict_scope`, `redact_args`) are explicit.
6. Runtime paths are untouched.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave25-obligations-step1.sh
```

## Expected outcome
- gate passes
- runtime code is untouched
- Wave25 Step2 can implement without reopening scope
