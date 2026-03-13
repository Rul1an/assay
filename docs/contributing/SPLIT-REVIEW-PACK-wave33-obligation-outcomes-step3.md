# SPLIT REVIEW PACK — Wave33 Obligation Outcomes Step3

## Intent
Close Wave33 with a docs+gate-only closure slice after bounded obligation-outcome normalization in Step2.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add new obligation execution semantics
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave33-obligation-outcomes-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave33-obligation-outcomes-step3.md`
- `scripts/ci/review-wave33-obligation-outcomes-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. Additive normalization fields remain present.
4. Baseline reason-code markers remain present.
5. Existing allow/deny behavior remains unchanged.
6. Existing obligation line remains intact.
7. No scope creep appears in runtime scope.
8. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave33-obligation-outcomes-step2-impl \
  bash scripts/ci/review-wave33-obligation-outcomes-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave33-obligation-outcomes-step3.sh
```
