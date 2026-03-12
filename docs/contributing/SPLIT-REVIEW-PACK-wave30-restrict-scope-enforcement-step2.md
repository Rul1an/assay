# SPLIT REVIEW PACK — Wave30 Restrict Scope Enforcement Step2

## Intent
Implement bounded runtime enforcement for `restrict_scope` on top of the Wave29 contract/evidence shape.

## Allowed implementation surface
- core MCP runtime enforcement paths
- core tests for deny/allow invariants
- Step2 docs/gate files

## What reviewers should verify
1. Diff is bounded to runtime enforcement + tests + Step2 docs/gate.
2. `restrict_scope` mismatches now deny deterministically.
3. `P_RESTRICT_SCOPE` is used for scope-enforcement deny paths.
4. Failure reasons are deterministic (`scope_target_missing`, `scope_target_mismatch`, `scope_match_mode_unsupported`, `scope_type_unsupported`).
5. Scope evidence remains additive and backward-compatible.
6. No rewrite/filter/redact behavior has been introduced.
7. Existing `log`/`alert`/`approval_required` line remains intact.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave30-restrict-scope-enforcement-step2.sh
```
