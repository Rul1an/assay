# Wave47 Pack Checks Step1 Review Pack (Freeze)

## Intent

Freeze the pack-check execution contract before any split of
`crates/assay-evidence/src/lint/packs/checks.rs`.

## Scope

- `docs/contributing/SPLIT-PLAN-wave47-pack-checks.md`
- `docs/contributing/SPLIT-CHECKLIST-wave47-pack-checks-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave47-pack-checks-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave47-pack-checks-step1.md`
- `scripts/ci/review-wave47-pack-checks-step1.sh`

## Non-goals

- no workflow changes
- no edits under `crates/assay-evidence/src/lint/packs/**`
- no edits under `crates/assay-evidence/tests/**`
- no edits under `packs/open/**`
- no `checks.rs` split in this wave
- no schema, spec, or pack-engine semantic cleanup

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave47-pack-checks-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -q -p assay-evidence --lib 'lint::packs::checks::tests::g3_authorization_check_uses_scoped_events_not_full_bundle' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::checks::tests::test_value_pointer' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::checks::tests::test_glob_matching' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_json_path_exists_value_equals_requires_exactly_one_path' -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test conditional_rule_fails_when_matching_event_lacks_required_path -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test event_field_present_respects_event_types_filter -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test json_path_exists_respects_event_types_filter -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_still_skips_for_security_pack -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_fails_for_compliance_pack -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp001_aligns_trust_basis_verified_and_pack_passes -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp001_aligns_trust_basis_absent_and_pack_fails -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp_followup_builtin_and_open_pack_are_exactly_equivalent -- --exact
cargo test -q -p assay-evidence --test a2a_discovery_card_followup_pack a2a_discovery_builtin_and_open_pack_are_exactly_equivalent -- --exact
cargo test -q -p assay-evidence --test a2a_discovery_card_followup_pack a2a_dc_001_fails_when_agent_card_visible_is_string_not_bool -- --exact
cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping a3_conditional_presence_rule_is_supported_in_engine_v1_1 -- --exact
cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping a3_conditional_presence_rule_fails_without_mandate_context -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to the Step1 allowlist.
2. Confirm `checks.rs`, `schema.rs`, test files, and open-pack content are frozen in this wave.
3. Confirm the plan keeps `schema.rs -> checks.rs` as the required order.
4. Confirm the move-map freezes dispatch, `json_path_exists` / `value_equals`, conditional,
   G3, and finding-emission semantics.
5. Confirm the reviewer script pins both `checks.rs` unit behavior and pack-level parity/execution tests.
