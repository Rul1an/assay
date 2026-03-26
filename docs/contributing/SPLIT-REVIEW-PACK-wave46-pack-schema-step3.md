# Wave46 Pack Schema Step3 Review Pack (Closure)

## Intent

Close the shipped Wave46 pack-schema split with docs/gates only and forbid post-Step2 redesign drift.

## Scope

- `docs/contributing/SPLIT-PLAN-wave46-pack-schema.md`
- `docs/contributing/SPLIT-CHECKLIST-wave46-pack-schema-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave46-pack-schema-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave46-pack-schema-step3.md`
- `scripts/ci/review-wave46-pack-schema-step3.sh`

## Non-goals

- no workflow changes
- no changes under `crates/assay-evidence/src/lint/packs/**`
- no changes under `crates/assay-evidence/tests/**`
- no changes under `packs/open/**`
- no new module cuts
- no schema validation, conditional-shape, parity, or error-meaning drift

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave46-pack-schema-step3.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_is_valid_pack_name' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_builtin_wins_over_local' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_local_invalid_yaml_fails' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_path_wins_over_builtin' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_supported_conditional_shape_parses' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_with_multiple_then_paths_is_unsupported' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_validation_requires_condition_object' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_validation_requires_then_object' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_json_path_exists_value_equals_requires_exactly_one_path' -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test conditional_rule_fails_when_matching_event_lacks_required_path -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test json_path_exists_respects_event_types_filter -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_still_skips_for_security_pack -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_fails_for_compliance_pack -- --exact
cargo test -q -p assay-evidence --test a2a_discovery_card_followup_pack a2a_discovery_builtin_and_open_pack_are_exactly_equivalent -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp_followup_builtin_and_open_pack_are_exactly_equivalent -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to the Step3 allowlist.
2. Confirm `crates/assay-evidence/src/lint/packs/**`, `crates/assay-evidence/tests/**`, and `packs/open/**` are frozen in this wave.
3. Confirm the plan records `#964` as shipped and bounds Step3 to closure only.
4. Confirm the move-map freezes the current module ownership and does not propose another split.
5. Confirm the reviewer script re-runs the pinned schema/loader/parity invariants.
