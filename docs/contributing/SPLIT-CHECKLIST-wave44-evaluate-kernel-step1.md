# Wave44 Evaluate Kernel Step1 Checklist (Freeze)

Scope lock:
- `docs/contributing/SPLIT-PLAN-wave44-evaluate-kernel.md`
- `docs/contributing/SPLIT-CHECKLIST-wave44-evaluate-kernel-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave44-evaluate-kernel-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave44-evaluate-kernel-step1.md`
- `scripts/ci/review-wave44-evaluate-kernel-step1.sh`
- no code edits under `crates/assay-core/src/mcp/tool_call_handler/**`
- no edits under `crates/assay-core/tests/**`
- no workflow edits

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in `crates/assay-core/src/mcp/tool_call_handler/**`
- hard fail untracked files in `crates/assay-core/src/mcp/tool_call_handler/**`
- hard fail tracked changes in `crates/assay-core/tests/**`
- hard fail untracked files in `crates/assay-core/tests/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted tests:
  - `mcp::tool_call_handler::tests::approval_required_missing_denies`
  - `mcp::tool_call_handler::tests::restrict_scope_target_missing_denies`
  - `mcp::tool_call_handler::tests::redact_args_target_missing_denies`
  - `tool_taxonomy_policy_match_handler_decision_event_records_classes`
  - `fulfillment_normalizes_outcomes_and_sets_policy_deny_path`
  - `classify_replay_diff_unchanged`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-wave44-evaluate-kernel-step1.sh` passes
- Step1 diff contains only the 5 allowlisted files
- no LOC drift in:
  - `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
  - `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
  - `crates/assay-core/tests/decision_emit_invariant.rs`
  - `crates/assay-core/tests/fulfillment_normalization.rs`
  - `crates/assay-core/tests/replay_diff_contract.rs`
  - `crates/assay-core/tests/tool_taxonomy_policy_match.rs`
