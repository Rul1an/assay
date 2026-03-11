# SPLIT REVIEW PACK - Wave26 Obligations Step1

## Intent
Freeze Wave26 scope before implementation by defining a bounded `alert` obligations execution contract.

This slice is docs + gate only.

It must not:
- change MCP runtime code
- change CLI normalization code
- change MCP server behavior
- change workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave26-obligations-alert.md`
- `docs/contributing/SPLIT-CHECKLIST-wave26-obligations-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave26-obligations-step1.md`
- `scripts/ci/review-wave26-obligations-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Wave26 execution scope is explicit (`log` + `alert`).
3. `legacy_warning` compatibility is explicit.
4. Non-goals (`approval_required`, `restrict_scope`, `redact_args`) are explicit.
5. Runtime paths are untouched.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave26-obligations-step1.sh
```

## Expected outcome
- gate passes
- runtime code is untouched
- scope is frozen cleanly
- Step2 can implement `alert` execution without reopening semantics
