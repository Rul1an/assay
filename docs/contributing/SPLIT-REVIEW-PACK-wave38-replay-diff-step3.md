# SPLIT REVIEW PACK - Wave38 Replay Diff Step3

## Intent
Close Wave38 with a docs+gate-only closure slice after bounded replay/diff basis and bucket implementation in Step2.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add new obligation types
- expand runtime enforcement semantics
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave38-replay-diff-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave38-replay-diff-step3.md`
- `scripts/ci/review-wave38-replay-diff-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. Replay-diff basis and bucket markers remain present.
4. Existing convergence + normalization markers remain present.
5. Existing deny-path separation remains present.
6. No non-goal scope creep appears in runtime scope.
7. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave38-replay-diff-step2-impl \
  bash scripts/ci/review-wave38-replay-diff-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave38-replay-diff-step3.sh
```

Expected outcome:
- Step3 adds no runtime behavior.
- closure remains diff-proof.
- promote can happen cleanly after stacked validation.
