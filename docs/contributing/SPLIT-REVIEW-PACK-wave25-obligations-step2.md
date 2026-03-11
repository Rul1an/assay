# SPLIT REVIEW PACK - Wave25 Obligations Step2

## Intent
Implement Wave25 Step2 as a bounded obligations execution slice.

This slice must:
- execute `log` obligations
- map `legacy_warning` to `log` execution
- add additive `obligation_outcomes` to decision events

This slice must not:
- add `approval_required` execution
- add `restrict_scope` execution
- add `redact_args` execution
- change workflow files

## Allowed files
- `crates/assay-core/src/mcp/mod.rs`
- `crates/assay-core/src/mcp/obligations.rs`
- `crates/assay-core/src/mcp/decision.rs`
- `crates/assay-core/src/mcp/proxy.rs`
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
- `crates/assay-core/tests/decision_emit_invariant.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave25-obligations-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave25-obligations-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave25-obligations-step2.md`
- `scripts/ci/review-wave25-obligations-step2.sh`

## What reviewers should verify
1. Runtime execution is limited to `log` only.
2. `legacy_warning` is mapped to `log` outcome.
3. Decision events include additive `obligation_outcomes`.
4. Existing event fields and allow/deny semantics remain stable.
5. No high-risk obligations execution markers appear.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave25-obligations-step2.sh
```
