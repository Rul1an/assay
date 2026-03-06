# SPLIT MOVE MAP - Wave8A Step2 (A2A)

Source hotspot: `crates/assay-adapter-a2a/src/lib.rs`.

## Move map

- `impl ProtocolAdapter for A2aAdapter` facade methods -> `crates/assay-adapter-a2a/src/lib.rs` (thin facade)
- adapter/protocol/capability constants + descriptor builders -> `crates/assay-adapter-a2a/src/adapter_impl/mod.rs`
- `convert` orchestration function -> `crates/assay-adapter-a2a/src/adapter_impl/convert.rs`
- `parse_packet`, `validate_protocol` -> `crates/assay-adapter-a2a/src/adapter_impl/parse.rs`
- `observed_version`, `validate_supported_version`, `parse_version` -> `crates/assay-adapter-a2a/src/adapter_impl/version.rs`
- `string_field`, `nested_string_field`, `nested_string_array_field`, `timestamp_field`, `default_time` -> `crates/assay-adapter-a2a/src/adapter_impl/fields.rs`
- `map_event_type`, `primary_id_for_event`, `count_unmapped_top_level_fields` -> `crates/assay-adapter-a2a/src/adapter_impl/mapping.rs`
- `build_payload`, `normalize_json` -> `crates/assay-adapter-a2a/src/adapter_impl/payload.rs`
- previous inline test module -> `crates/assay-adapter-a2a/src/adapter_impl/tests.rs`

## Invariants

- No public API changes.
- No event mapping or error-contract changes.
- No fixture/test scenario removal.
