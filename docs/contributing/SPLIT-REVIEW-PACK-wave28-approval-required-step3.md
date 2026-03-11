# SPLIT REVIEW PACK — Wave28 Approval Required Step3

## Intent
Close Wave28 with a docs+gate-only closure slice after the bounded implementation of `approval_required` runtime enforcement.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add approval UI/case-management
- add external approval services
- add `restrict_scope` or `redact_args`
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave28-approval-required-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave28-approval-required-step3.md`
- `scripts/ci/review-wave28-approval-required-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. Approval artifact/evidence markers remain present.
4. `approval_required` runtime enforcement remains bounded.
5. Missing/expired/mismatch approval still yields deny behavior.
6. Existing `log`/`alert`/`legacy_warning` line remains intact.
7. No scope creep appears in runtime scope.
8. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave28-approval-required-step2-impl \
  bash scripts/ci/review-wave28-approval-required-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave28-approval-required-step3.sh
```

## Expected outcome
- Step3 adds no runtime behavior
- closure remains diff-proof
- promote can happen cleanly after stacked validation
