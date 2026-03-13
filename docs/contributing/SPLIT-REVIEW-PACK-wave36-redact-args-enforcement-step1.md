# SPLIT REVIEW PACK — Wave36 Redact Args Enforcement Step1

## Intent
Freeze a bounded hardening contract for `redact_args` runtime enforcement before any Step2 tightening changes.

This slice is docs + gate only.

It must not:
- change MCP runtime behavior
- change CLI normalization behavior
- change MCP server behavior
- widen redact semantics to global policy scope
- add DLP/PII workflow integrations
- touch workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave36-redact-args-enforcement.md`
- `docs/contributing/SPLIT-CHECKLIST-wave36-redact-args-enforcement-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave36-redact-args-enforcement-step1.md`
- `scripts/ci/review-wave36-redact-args-enforcement-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. Redact enforcement hardening scope is explicit.
3. Frozen redaction failure classes are explicit.
4. Deterministic normalization alignment (`reason_code`, `enforcement_stage`, `normalization_version`) is explicit.
5. Additive redact evidence fields are explicit.
6. Runtime paths are untouched.
7. No scope expansion to global redaction semantics or DLP integrations.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave36-redact-args-enforcement-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- hardening contract is frozen cleanly
- Step2 can tighten determinism without reopening scope
