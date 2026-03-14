# SPLIT REVIEW PACK - Wave40 Deny Evidence Step3

## Intent
Close Wave40 with a docs+gate-only closure slice after bounded deny/fail-closed evidence convergence implementation in Step2.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add runtime enforcement behavior changes
- add policy-language/control-plane/auth transport scope
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave40-deny-evidence-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave40-deny-evidence-step3.md`
- `scripts/ci/review-wave40-deny-evidence-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. Wave40 deny-convergence markers remain present.
4. Deterministic deny precedence markers remain present.
5. Existing replay/decision markers remain present.
6. No runtime behavior scope creep appears in closure slice.
7. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave40-deny-evidence-step2-impl-v2 \
  bash scripts/ci/review-wave40-deny-evidence-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave40-deny-evidence-step3.sh
```

Expected outcome:
- Step3 adds no runtime behavior.
- closure remains diff-proof.
- promote can happen cleanly after stacked validation.
