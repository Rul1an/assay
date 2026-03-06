# SPLIT MOVE MAP - Wave8B Step2 (UCP)

Source hotspot: `crates/assay-adapter-ucp/src/lib.rs`.

## Move map

- `impl ProtocolAdapter for UcpAdapter` facade methods -> `crates/assay-adapter-ucp/src/lib.rs` (thin facade)
- adapter/protocol/capability constants + descriptor builders -> `crates/assay-adapter-ucp/src/adapter_impl/mod.rs`
- `convert` orchestration function -> `crates/assay-adapter-ucp/src/adapter_impl/convert.rs`
- `parse_packet`, `validate_protocol` -> `crates/assay-adapter-ucp/src/adapter_impl/parse.rs`
- `observed_version`, `validate_supported_version` -> `crates/assay-adapter-ucp/src/adapter_impl/version.rs`
- `string_field`, `nested_string_field`, `timestamp_field`, `default_time` -> `crates/assay-adapter-ucp/src/adapter_impl/fields.rs`
- `map_event_type`, `primary_id_for_event`, `count_unmapped_top_level_fields` -> `crates/assay-adapter-ucp/src/adapter_impl/mapping.rs`
- `build_payload`, `normalized_object_field`, `normalize_json` -> `crates/assay-adapter-ucp/src/adapter_impl/payload.rs`
- previous inline test module -> `crates/assay-adapter-ucp/src/adapter_impl/tests.rs`

## Invariants

- No public API changes.
- No event mapping or error-contract changes.
- No fixture/test scenario removal.
