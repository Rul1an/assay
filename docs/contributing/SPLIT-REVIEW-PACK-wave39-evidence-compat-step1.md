# SPLIT REVIEW PACK - Wave39 Evidence Compat Step1

## Intent
Freeze a bounded replay-facing evidence compatibility contract before any additional runtime capability.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add runtime enforcement changes
- add policy language expansion
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave39-evidence-compat-normalization.md`
- `docs/contributing/SPLIT-CHECKLIST-wave39-evidence-compat-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave39-evidence-compat-step1.md`
- `scripts/ci/review-wave39-evidence-compat-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Replay/evidence compatibility fields are explicitly frozen.
3. Deterministic classification precedence is explicit.
4. Legacy fallback semantics are additive and explicit.
5. Runtime paths are untouched.
6. No scope expansion beyond compatibility contract freeze.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave39-evidence-compat-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- compatibility contract is frozen cleanly
- Step2 can implement additive replay/evidence fields without reopening semantics
