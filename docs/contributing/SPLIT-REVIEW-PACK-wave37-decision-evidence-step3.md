# SPLIT REVIEW PACK - Wave37 Decision Evidence Convergence Step3

## Intent
Close Wave37 with a docs+gate-only closure slice after bounded decision/evidence convergence implementation.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add new obligations or runtime capabilities
- expand policy-engine scope
- add UI/control-plane/auth transport work
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave37-decision-evidence-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave37-decision-evidence-step3.md`
- `scripts/ci/review-wave37-decision-evidence-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. Convergence fields remain present and additive.
4. Deterministic deny and obligation classification remains present.
5. Existing fulfillment normalization remains intact.
6. Existing obligation execution markers remain present.
7. No non-goal scope creep appears in runtime scope.
8. Pinned tests still pass.

## Reviewer commands

### Against origin/main after sync (primary)
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave37-decision-evidence-step3.sh
```

### Optional: against stacked Step2 base (only if ancestry is preserved)
```bash
BASE_REF=origin/codex/wave37-decision-evidence-convergence-step2-impl \
  bash scripts/ci/review-wave37-decision-evidence-step3.sh
```
