# SPLIT REVIEW PACK - Wave38 Replay Diff Step1

## Intent
Freeze a bounded replay/diff contract for deterministic decision evidence comparisons.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add runtime capability
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave38-replay-diff-contract.md`
- `docs/contributing/SPLIT-CHECKLIST-wave38-replay-diff-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave38-replay-diff-step1.md`
- `scripts/ci/review-wave38-replay-diff-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Replay basis fields are explicit and deterministic.
3. Diff buckets are explicit and bounded.
4. Runtime paths are untouched.
5. No scope expansion appears in this wave.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave38-replay-diff-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- replay/diff contract is frozen cleanly
- Step2 can implement comparison logic without reopening semantics
