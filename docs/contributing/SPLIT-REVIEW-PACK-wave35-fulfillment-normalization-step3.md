# SPLIT REVIEW PACK - Wave35 Fulfillment Normalization Step3

## Intent
Close Wave35 with a docs+gate-only closure slice after bounded fulfillment-normalization implementation.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add new obligations
- expand policy-engine scope
- add UI/control-plane/auth transport work
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave35-fulfillment-normalization-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave35-fulfillment-normalization-step3.md`
- `scripts/ci/review-wave35-fulfillment-normalization-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. Normalized fulfillment fields remain present and additive.
4. Deterministic defaults and decision-path mapping remain present.
5. Policy-deny vs fail-closed-deny separation remains explicit.
6. Existing obligation execution markers remain present.
7. No non-goal scope creep appears in runtime scope.
8. Pinned tests still pass.

## Reviewer commands

### Against origin/main after sync (primary)
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave35-fulfillment-normalization-step3.sh
```

### Optional: against stacked Step2 base (only if ancestry is preserved)
```bash
BASE_REF=origin/codex/wave35-fulfillment-normalization-step2-impl \
  bash scripts/ci/review-wave35-fulfillment-normalization-step3.sh
```
