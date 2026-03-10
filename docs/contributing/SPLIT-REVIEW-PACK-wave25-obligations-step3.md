# SPLIT REVIEW PACK - Wave25 Obligations Step3

## Intent
Close Wave25 with a docs+gate-only closure slice after Step2 implementation.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add high-risk obligations execution
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave25-obligations-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave25-obligations-step3.md`
- `scripts/ci/review-wave25-obligations-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns Step2 structural invariants.
3. `allow_with_obligations`, `execute_log_only`, and `legacy_warning` markers remain present.
4. `obligation_outcomes` and status markers remain present.
5. No high-risk obligations execution markers appear in runtime scope.
6. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave25-obligations-log-step2-impl \
  bash scripts/ci/review-wave25-obligations-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave25-obligations-step3.sh
```

## Expected outcome
- Step3 adds no runtime behavior
- closure remains diff-proof
- promote can happen cleanly after stacked validation
