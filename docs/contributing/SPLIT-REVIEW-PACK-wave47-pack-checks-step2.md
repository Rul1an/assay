# Wave47 Pack Checks Step2 Review Pack

## Intent

Mechanically split `crates/assay-evidence/src/lint/packs/checks.rs` behind a stable facade,
without changing check dispatch, finding semantics, or pack-visible runtime behavior.

## Scope

- `crates/assay-evidence/src/lint/packs/checks.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/mod.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/event.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/json_path.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/conditional.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/manifest.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/finding.rs`
- `docs/contributing/SPLIT-PLAN-wave47-pack-checks.md`
- `docs/contributing/SPLIT-CHECKLIST-wave47-pack-checks-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave47-pack-checks-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave47-pack-checks-step2.md`
- `scripts/ci/review-wave47-pack-checks-step2.sh`

## Non-goals

- no workflow changes
- no edits under `crates/assay-evidence/tests/**`
- no edits under `packs/open/**`
- no edits to `schema.rs` or `schema_next/*`
- no new check types, engine bump, or spec expansion
- no finding wording cleanup or dispatch redesign

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave47-pack-checks-step2.sh
```

Gate includes:

```bash
cargo fmt --all --check
cargo clippy -q -p assay-evidence --all-targets -- -D warnings
cargo check -q -p assay-evidence
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

1. Confirm the diff is limited to `checks.rs`, `checks_next/*`, and Step2 docs/script.
2. Confirm `schema.rs`, tests, and `packs/open/**` remain untouched.
3. Confirm `checks.rs` is now a thin facade with stable entrypoints and inline tests.
4. Confirm the move-map treats this as family-based relocation, not redesign.
5. Confirm the reviewer script pins both `checks.rs` unit behavior and pack-level parity/execution tests.
