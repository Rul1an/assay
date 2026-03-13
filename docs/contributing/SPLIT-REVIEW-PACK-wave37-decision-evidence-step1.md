# SPLIT REVIEW PACK — Wave37 Decision Evidence Convergence Step1

## Intent
Freeze a bounded convergence contract for decision/evidence outcomes before any Step2 implementation changes.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add new runtime capability
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave37-decision-evidence-convergence.md`
- `docs/contributing/SPLIT-CHECKLIST-wave37-decision-evidence-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave37-decision-evidence-step1.md`
- `scripts/ci/review-wave37-decision-evidence-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Canonical outcome taxonomy is explicit.
3. Deterministic classification semantics are explicit.
4. Additive convergence evidence fields are explicit.
5. Downstream compatibility rules are explicit.
6. Runtime paths are untouched.
7. No capability expansion is introduced in this wave.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave37-decision-evidence-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- convergence contract is frozen cleanly
- Step2 can implement mapping/evidence convergence without reopening scope
