# SPLIT REVIEW PACK - Wave32 Redact Args Enforcement Step1

## Intent
Freeze the bounded runtime enforcement contract for `redact_args` before implementation.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add broad/global redaction semantics
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave32-redact-args-enforcement.md`
- `docs/contributing/SPLIT-CHECKLIST-wave32-redact-args-enforcement-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave32-redact-args-enforcement-step1.md`
- `scripts/ci/review-wave32-redact-args-enforcement-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Redaction validity checks are explicit.
3. Missing/unsupported/not-applied deny behavior is explicit.
4. Additive redaction evidence fields are explicit.
5. Runtime paths are untouched.
6. No scope expansion beyond bounded enforcement contract.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave32-redact-args-enforcement-step1.sh
```

## Expected outcome
- gate passes
- runtime code is untouched
- redaction enforcement contract is frozen cleanly
- Step2 can implement bounded enforcement without reopening semantics
