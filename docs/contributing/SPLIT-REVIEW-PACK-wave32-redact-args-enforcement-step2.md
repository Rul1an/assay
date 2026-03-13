# SPLIT REVIEW PACK — Wave32 Redact Args Enforcement Step2

## Intent
Implement bounded runtime enforcement for `redact_args` on top of the Wave31 contract/evidence shape.

## Allowed implementation surface
- core MCP runtime enforcement paths
- core tests for deny/allow invariants
- Step2 docs/gate files

## What reviewers should verify
1. Diff is bounded to runtime enforcement + tests + Step2 docs/gate.
2. `redact_args` failures now deny deterministically.
3. `P_REDACT_ARGS` is used for redaction-enforcement deny paths.
4. Failure reasons are deterministic:
   - `redaction_target_missing`
   - `redaction_mode_unsupported`
   - `redaction_scope_unsupported`
   - `redaction_apply_failed`
5. Redaction evidence remains additive and backward-compatible.
6. Existing `log`/`alert`/`approval_required`/`restrict_scope` line remains intact.
7. No scope creep into broad/global redaction behavior.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave32-redact-args-enforcement-step2.sh
```
