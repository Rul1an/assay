# SPLIT REVIEW PACK — Wave32 Redact Args Enforcement Step3

## Intent
Close Wave32 with a docs+gate-only closure slice after bounded `redact_args` runtime enforcement in Step2.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add broad/global redaction semantics
- add PII detection/external DLP integrations
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave32-redact-args-enforcement-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave32-redact-args-enforcement-step3.md`
- `scripts/ci/review-wave32-redact-args-enforcement-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns the same structural invariants from Step2.
3. `P_REDACT_ARGS` and `validate_redact_args` remain present.
4. Redaction failure reasons remain deterministic.
5. Redaction evidence remains additive and backward-compatible.
6. Existing `log`/`alert`/`approval_required`/`restrict_scope` line remains intact.
7. No scope creep appears in runtime scope.
8. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base
```bash
BASE_REF=origin/codex/wave32-redact-args-enforcement-step2-impl \
  bash scripts/ci/review-wave32-redact-args-enforcement-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave32-redact-args-enforcement-step3.sh
```
