# SPLIT REVIEW PACK - Wave29 Restrict Scope Step2

## Intent
Implement a bounded Step2 for Wave29 by adding typed `restrict_scope` contract shape and additive evidence fields, without runtime enforcement.

This slice must:
- add typed `restrict_scope` shape fields:
  - `scope_type`
  - `scope_value`
  - `scope_match_mode`
  - `scope_evaluation_state`
  - `scope_failure_reason`
- add additive evidence fields:
  - `restrict_scope_present`
  - `restrict_scope_target`
  - `restrict_scope_match`
  - `restrict_scope_reason`
- preserve existing `log`/`alert`/`approval_required` behavior
- keep old event consumers compatible

This slice must not:
- add runtime enforcement of `restrict_scope`
- add `restrict_scope` deny behavior
- add arg rewriting/filtering/redaction
- touch workflow files

## Allowed files
- `crates/assay-core/src/mcp/policy/mod.rs`
- `crates/assay-core/src/mcp/policy/engine.rs`
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
- `crates/assay-core/src/mcp/decision.rs`
- `crates/assay-core/src/mcp/proxy.rs`
- `crates/assay-core/src/mcp/obligations.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
- `crates/assay-core/tests/decision_emit_invariant.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave29-restrict-scope-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave29-restrict-scope-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave29-restrict-scope-step2.md`
- `scripts/ci/review-wave29-restrict-scope-step2.sh`

## What reviewers should verify
1. Diff is allowlist-bounded and workflow-clean.
2. Restrict-scope shape fields are present in policy/runtime/event path.
3. Additive scope evidence fields are present.
4. Existing obligations and approval behavior remains stable.
5. No `restrict_scope` enforcement path is introduced.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave29-restrict-scope-step2.sh
```
