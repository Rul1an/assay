# SPLIT REVIEW PACK — Wave31 Redact Args Step3

## Intent
Close Wave31 with a docs+gate-only closure slice after the bounded Step2 contract/evidence implementation for `redact_args`.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- introduce `redact_args` runtime mutation/enforcement
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave31-redact-args-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave31-redact-args-step3.md`
- `scripts/ci/review-wave31-redact-args-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. Redaction shape and additive evidence markers remain present.
4. `redact_args` remains contract-only (no deny/rewrite path).
5. Existing `log`/`alert`/`approval_required`/`restrict_scope` line remains intact.
6. No scope creep appears in runtime scope.
7. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave31-redact-args-step2-impl \
  bash scripts/ci/review-wave31-redact-args-step3.sh
```

### Against `origin/main` after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave31-redact-args-step3.sh
```
