# SPLIT REVIEW PACK — Wave31 Redact Args Step1

## Intent
Freeze the `redact_args` contract/evidence shape before any runtime redaction execution.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add runtime payload mutation/redaction
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave31-redact-args.md`
- `docs/contributing/SPLIT-CHECKLIST-wave31-redact-args-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave31-redact-args-step1.md`
- `scripts/ci/review-wave31-redact-args-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Typed `redact_args` shape is explicit.
3. Redactable zones are explicit.
4. Additive redaction evidence fields are explicit.
5. Runtime paths are untouched.
6. No runtime redaction execution is introduced.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave31-redact-args-step1.sh
```

## Expected outcome
- gate passes
- runtime code is untouched
- redact_args contract/evidence is frozen cleanly
- Step2 can implement shape/evidence without reopening semantics
