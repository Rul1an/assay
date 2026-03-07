# Agentic Step2 Checklist (Mechanical Split)

Scope lock:
- mechanical split only
- no public API changes
- no behavior changes
- no workflow changes

## Goal

Split `crates/assay-core/src/agentic/mod.rs` into facade + internal modules while
preserving behavior and public surface.

## Target files

- `crates/assay-core/src/agentic/mod.rs`
- `crates/assay-core/src/agentic/builder.rs`
- `crates/assay-core/src/agentic/policy_helpers.rs`
- `crates/assay-core/src/agentic/tests/mod.rs`
- `docs/contributing/SPLIT-CHECKLIST-agentic-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-agentic-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-agentic-step2.md`
- `scripts/ci/review-agentic-step2.sh`

## Hard gates

- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib agentic::tests::test_deduplication -- --exact`
- `cargo test -p assay-core --lib agentic::tests::test_detect_policy_shape -- --exact`
- `cargo test -p assay-core --lib agentic::tests::test_tool_poisoning_action_uses_assay_config_not_policy -- --exact`

## Invariants

- facade `mod.rs` non-empty LOC `<= 220`
- exactly one non-comment call site for `builder::build_suggestions_impl(`
- no helper function definitions in `mod.rs`
- no impl blocks in `mod.rs` for:
  - `AgenticCtx`, `SuggestedAction`, `SuggestedPatch`, `RiskLevel`, `JsonPatchOp`
- public surface unchanged in `mod.rs`:
  - `RiskLevel`, `SuggestedAction`, `SuggestedPatch`, `JsonPatchOp`, `AgenticCtx`, `build_suggestions`
- `builder.rs` exports no public API except `pub(crate) fn build_suggestions_impl`
- original test names remain present:
  - `test_deduplication`
  - `test_unknown_tool_action_only`
  - `test_rename_field_patch`
  - `test_detect_policy_shape`
  - `test_tool_poisoning_action_uses_assay_config_not_policy`

## Diff controls

- allowlist-only (`crates/assay-core/src/agentic/**` + Step2 docs/script)
- workflow-ban (`.github/workflows/*`)

## Definition of done

- reviewer script passes with `BASE_REF=origin/codex/wave12-agentic-step1-freeze`
- no out-of-scope file changes
