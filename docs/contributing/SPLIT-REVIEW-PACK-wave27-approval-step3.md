# SPLIT REVIEW PACK - Wave27 Approval Step3

## Intent
Close Wave27 with a docs+gate-only Step3 closure slice after Step2 approval artifact shape implementation.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add approval enforcement
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave27-approval-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave27-approval-step3.md`
- `scripts/ci/review-wave27-approval-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns Step2 structural invariants.
3. Approval artifact/data-shape markers remain present.
4. Approval evidence markers remain present.
5. No approval enforcement markers appear in runtime scope.
6. Pinned tests still pass.

## Reviewer commands

### Against stacked base
```bash
BASE_REF=origin/main bash scripts/ci/review-wave27-approval-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main bash scripts/ci/review-wave27-approval-step3.sh
```
