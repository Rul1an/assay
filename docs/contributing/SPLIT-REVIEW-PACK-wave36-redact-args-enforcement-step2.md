# SPLIT REVIEW PACK - Wave36 Redact Args Enforcement Step2

## Intent
Implement bounded runtime redaction for `redact_args` while preserving deterministic evidence and backward-compatible event shape.

This slice must:
- execute runtime redaction in handler path
- preserve deterministic deny reasons for frozen redaction failure classes
- keep evidence and `obligation_outcomes` additive and normalized

This slice must not:
- add new obligation types
- broaden redact semantics into global policy workflows
- add PII engines or external DLP integrations
- add UI/control-plane/auth transport work
- touch workflow files

## Reviewer focus
1. Diff stays inside bounded runtime/test/docs/gate scope.
2. Runtime redaction execution is real (not metadata-only).
3. Failure classes are deterministic and mapped to `P_REDACT_ARGS` deny path.
4. Event/evidence shape remains additive and compatible.
5. Existing obligation lines remain stable (`log`, `alert`, `approval_required`, `restrict_scope`).

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave36-redact-args-enforcement-step2.sh
```
