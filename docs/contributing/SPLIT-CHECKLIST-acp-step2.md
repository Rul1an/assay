# ACP Step2 Checklist (Mechanical Split)

Scope lock:
- `crates/assay-adapter-acp/src/lib.rs`
- `crates/assay-adapter-acp/src/adapter_impl/**`
- `crates/assay-adapter-acp/src/tests/mod.rs`
- Step2 docs + reviewer script only
- no workflow changes

## Required outputs

- `docs/contributing/SPLIT-CHECKLIST-acp-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-acp-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-acp-step2.md`
- `scripts/ci/review-acp-step2.sh`

## Mechanical invariants

- `lib.rs` is facade-only with exactly one wrapper call to `adapter_impl::convert_impl(...)`.
- `lib.rs` contains no inline `mod tests { ... }` block.
- `lib.rs` contains no mapping `match` logic.
- public surface in `lib.rs` is unchanged (`AcpAdapter` + `ProtocolAdapter` contract).
- implementation modules expose only `pub(crate)` items.
- test names remain unchanged (11 existing ACP tests).

## Must-survive tests

- `strict_happy_fixture_emits_deterministic_event`
- `strict_checkout_fixture_preserves_attributes_without_lossiness`
- `strict_attribute_order_normalizes_payload_but_keeps_raw_byte_hash_boundary`
- `strict_missing_required_field_fails_with_measurement_error`
- `lenient_invalid_event_type_emits_generic_event_and_lossiness`
- `malformed_json_fails_in_all_modes`
- `oversized_payload_fails_measurement_contract`
- `invalid_utf8_payload_fails_measurement_contract`
- `excessive_json_depth_fails_measurement_contract`
- `excessive_array_length_fails_measurement_contract`
- `strict_unknown_top_level_fields_account_for_lossiness`

## Gate requirements

- `cargo fmt --check`
- `cargo clippy -p assay-adapter-acp -p assay-adapter-api --all-targets -- -D warnings`
- `cargo test -p assay-adapter-acp`
- allowlist-only diff
- workflow-ban
- facade + visibility invariants

## Definition of done

- reviewer script passes with `BASE_REF=origin/main`
- Step2 diff stays inside Step2 allowlist
- ACP tests remain green with unchanged test names
