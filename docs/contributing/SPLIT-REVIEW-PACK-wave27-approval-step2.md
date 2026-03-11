# SPLIT REVIEW PACK - Wave27 Approval Step2

## Intent
Implement a bounded Step2 for Wave27 by adding approval artifact/data shape and additive approval evidence fields.

This slice must:
- add approval artifact fields (`approval_id`, `approver`, `issued_at`, `expires_at`, `scope`, `bound_tool`, `bound_resource`)
- add approval freshness shape (`approval_freshness`)
- keep evidence additive and backward-compatible
- preserve existing typed decision + obligations behavior

This slice must not:
- enforce `approval_required` at runtime
- block on missing/expired approval
- add approval UI/case-management
- add external approval service dependencies
- touch workflow files

## Allowed files
- `crates/assay-core/src/mcp/policy/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
- `crates/assay-core/src/mcp/decision.rs`
- `crates/assay-core/src/mcp/proxy.rs`
- `crates/assay-core/tests/decision_emit_invariant.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave27-approval-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave27-approval-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave27-approval-step2.md`
- `scripts/ci/review-wave27-approval-step2.sh`

## What reviewers should verify
1. Diff is allowlist-bounded and workflow-clean.
2. Approval artifact fields are represented in runtime policy/event path.
3. Freshness/expiry and tool/resource binding are explicitly modeled.
4. Approval evidence fields are additive.
5. Existing typed decision and obligations behavior remains stable.
6. No approval enforcement is introduced in this wave.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave27-approval-step2.sh
```
