# Wave46 Plan — `lint/packs/schema.rs` Kernel Split

## Goal

Split `crates/assay-evidence/src/lint/packs/schema.rs` behind a stable schema facade with zero pack
validation drift and no built-in/open pack compatibility drift.

Current hotspot baseline on `origin/main @ ee9cd502`:
- `crates/assay-evidence/src/lint/packs/schema.rs`: `844` LOC
- `crates/assay-evidence/src/lint/packs/checks.rs`: `785` LOC
- `crates/assay-evidence/tests/pack_engine_conditional_test.rs`: contract companion
- `crates/assay-evidence/tests/a2a_discovery_card_followup_pack.rs`: built-in/open parity companion
- `crates/assay-evidence/tests/mcp_signal_followup_pack.rs`: built-in/open parity companion

## Step1 (freeze)

Branch: `codex/wave46-pack-schema-step1` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave46-pack-schema.md`
- `docs/contributing/SPLIT-CHECKLIST-wave46-pack-schema-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave46-pack-schema-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave46-pack-schema-step1.md`
- `scripts/ci/review-wave46-pack-schema-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `crates/assay-evidence/src/lint/packs/**`
- no edits under `crates/assay-evidence/tests/**`
- no edits under `packs/open/**`
- no workflow edits
- no loader/check execution changes

Step1 gate:
- allowlist-only diff (the 5 Step1 files)
- workflow-ban (`.github/workflows/*`)
- hard fail on tracked changes in `crates/assay-evidence/src/lint/packs/**`
- hard fail on untracked files in `crates/assay-evidence/src/lint/packs/**`
- hard fail on tracked changes in `crates/assay-evidence/tests/**`
- hard fail on untracked files in `crates/assay-evidence/tests/**`
- hard fail on tracked changes in `packs/open/**`
- hard fail on untracked files in `packs/open/**`
- `cargo fmt --check`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- targeted tests:
  - `cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_is_valid_pack_name' -- --exact`
  - `cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_builtin_wins_over_local' -- --exact`
  - `cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_local_invalid_yaml_fails' -- --exact`
  - `cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_path_wins_over_builtin' -- --exact`
  - `cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_supported_conditional_shape_parses' -- --exact`
  - `cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_with_multiple_then_paths_is_unsupported' -- --exact`
  - `cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_validation_requires_condition_object' -- --exact`
  - `cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_validation_requires_then_object' -- --exact`
  - `cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_json_path_exists_value_equals_requires_exactly_one_path' -- --exact`
  - `cargo test -q -p assay-evidence --test pack_engine_conditional_test conditional_rule_fails_when_matching_event_lacks_required_path -- --exact`
  - `cargo test -q -p assay-evidence --test pack_engine_conditional_test json_path_exists_respects_event_types_filter -- --exact`
  - `cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_still_skips_for_security_pack -- --exact`
  - `cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_fails_for_compliance_pack -- --exact`
  - `cargo test -q -p assay-evidence --test a2a_discovery_card_followup_pack a2a_discovery_builtin_and_open_pack_are_exactly_equivalent -- --exact`
  - `cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp_followup_builtin_and_open_pack_are_exactly_equivalent -- --exact`

## Frozen public surface

Wave46 freezes the expectation that Step2 keeps these schema-layer types and contracts stable in
meaning:
- `PackDefinition`
- `PackRequirements`
- `PackRule`
- `CheckDefinition`
- `PackValidationError`
- `SupportedConditionalCheck`
- `SupportedConditionalClause`

Step2 may reorganize internal ownership behind `schema.rs`, but must not redefine:
- pack YAML parse/validation behavior
- `json_path_exists` and `value_equals` validation rules
- the single-path requirement for `value_equals`
- conditional-shape acceptance vs unsupported classification
- loader-visible built-in/open pack loadability and parity assumptions
- validation error category or reason-string meaning

## Step2 (mechanical split preview)

Branch: `codex/wave46-pack-schema-step2` (base: `main`)

Target layout:
- `crates/assay-evidence/src/lint/packs/schema.rs` (thin facade + stable exports)
- `crates/assay-evidence/src/lint/packs/schema_next/mod.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/types.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/serde.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/validation.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/conditional.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/errors.rs`

Step2 scope:
- `crates/assay-evidence/src/lint/packs/schema.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/*`
- `docs/contributing/SPLIT-PLAN-wave46-pack-schema.md`
- `docs/contributing/SPLIT-CHECKLIST-wave46-pack-schema-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave46-pack-schema-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave46-pack-schema-step2.md`
- `scripts/ci/review-wave46-pack-schema-step2.sh`

Step2 principles:
- 1:1 body moves
- stable schema types and validation behavior
- no `json_path_exists` / `value_equals` drift
- no conditional-shape or unsupported-path drift
- no loader or built-in/open pack parity drift
- no edits under `crates/assay-evidence/tests/**`
- no workflow edits

## Step3 (closure)

Docs+gate-only closure slice that re-runs Step2 invariants and limits any follow-up to
micro-cleanup only.

## Promote

Stacked chain:
- Step1 -> `main`
- Step2 -> Step1
- Step3 -> Step2

Final promote PR to `main` from Step3 once the chain is clean.

## Reviewer notes

This wave must remain pack-schema split planning only.

Primary failure modes:
- moving validation semantics while claiming a mechanical split
- relaxing `json_path_exists.value_equals` or conditional-shape rules during decomposition
- changing loader-visible pack acceptance behavior via schema-only churn
- mixing `checks.rs` execution logic into the `schema.rs` wave
