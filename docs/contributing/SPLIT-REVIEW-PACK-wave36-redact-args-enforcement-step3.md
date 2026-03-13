# SPLIT REVIEW PACK - Wave36 Redact Args Enforcement Step3

## Intent
Close Wave36 with a docs+gate-only closure slice after bounded runtime `redact_args` enforcement landed.

This slice must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- add PII/DLP scope
- add UI/control-plane scope
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-CHECKLIST-wave36-redact-args-enforcement-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave36-redact-args-enforcement-step3.md`
- `scripts/ci/review-wave36-redact-args-enforcement-step3.sh`

## What reviewers should verify
1. Diff is docs+script only.
2. Step3 reruns Step2 structural invariants.
3. Runtime redaction execution markers remain present.
4. Deterministic redaction failure mapping remains present.
5. Additive redaction evidence markers remain present.
6. Existing obligation lines remain intact.
7. No scope creep appears in runtime scope.
8. Pinned tests still pass.

## Reviewer commands

### Against stacked Step2 base (if used)
```bash
BASE_REF=origin/codex/wave36-redact-args-enforcement-step2-impl \
  bash scripts/ci/review-wave36-redact-args-enforcement-step3.sh
```

### Against origin/main after sync
```bash
BASE_REF=origin/main \
  bash scripts/ci/review-wave36-redact-args-enforcement-step3.sh
```

Expected outcome:
- Step3 adds no runtime behavior
- closure remains diff-proof
- promote can happen cleanly after stacked validation
