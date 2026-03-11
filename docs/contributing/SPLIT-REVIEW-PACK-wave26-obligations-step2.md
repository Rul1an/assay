# SPLIT REVIEW PACK - Wave26 Obligations Step2

## Intent
Implement Wave26 Step2 as a bounded obligations execution slice for `alert`.

This slice must:
- execute `alert` obligations in the existing bounded runtime path
- preserve existing `log` behavior
- preserve `legacy_warning` -> `log` compatibility
- keep `obligation_outcomes` additive

This slice must not:
- add `approval_required` execution
- add `restrict_scope` execution
- add `redact_args` execution
- add external incident/case-management integration
- change workflow files

## Allowed files
- `crates/assay-core/src/mcp/obligations.rs`
- `crates/assay-core/src/mcp/policy/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave26-obligations-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave26-obligations-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave26-obligations-step2.md`
- `scripts/ci/review-wave26-obligations-step2.sh`

## What reviewers should verify
1. Runtime execution remains bounded and non-blocking.
2. `alert` obligations are executed and recorded in outcomes.
3. `legacy_warning` still maps to `log` outcomes.
4. Decision/event compatibility remains additive.
5. No high-risk obligations execution markers appear.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave26-obligations-step2.sh
```
