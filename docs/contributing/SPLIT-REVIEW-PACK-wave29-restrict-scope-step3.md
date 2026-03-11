# SPLIT REVIEW PACK — Wave29 Restrict Scope Step3

## Intent
Close Wave29 with a docs+gate-only closure slice after the bounded implementation of `restrict_scope` contract/evidence shape.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add `restrict_scope` runtime enforcement
- add arg rewriting/filtering/redaction behavior
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave29-restrict-scope-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave29-restrict-scope-step3.md`
- `scripts/ci/review-wave29-restrict-scope-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. Restrict-scope contract/evidence markers remain present.
4. Restrict-scope behavior remains contract-only and non-enforcing.
5. Existing `log`/`alert`/`approval_required`/`legacy_warning` line remains intact.
6. No scope creep appears in runtime scope.
7. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave29-restrict-scope-step2-impl \
  bash scripts/ci/review-wave29-restrict-scope-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave29-restrict-scope-step3.sh
```

## Expected outcome
- Step3 adds no runtime behavior
- closure remains diff-proof
- promote can happen cleanly after stacked validation
