# SPLIT-MOVE-MAP — Wave46 Step1 — `lint/packs/schema.rs`

## Goal

Freeze the pack-schema contract before any mechanical split of
`crates/assay-evidence/src/lint/packs/schema.rs`.

## Current hotspot boundary

- `crates/assay-evidence/src/lint/packs/schema.rs`
  currently co-locates pack metadata types, serde shape, validation rules, conditional support,
  and validation error semantics.
- `crates/assay-evidence/src/lint/packs/checks.rs`
  consumes these definitions and `checks.rs` is explicitly **out of scope** for this wave.

## Frozen behavior boundaries

- pack YAML/schema parse and validation behavior
- `json_path_exists` contract
- `json_path_exists.value_equals` single-path rule
- conditional-shape validation and unsupported-shape classification
- built-in/open pack loadability and parity assumptions
- validation error category and reason-string meaning
- spec coupling around engine/schema version requirements

## Intended Step2 ownership split

- `schema.rs`: thin facade and stable exports
- `schema_next/types.rs`: pack metadata and rule types
- `schema_next/serde.rs`: serde helpers and compatibility parsing
- `schema_next/validation.rs`: pack/rule validation orchestration
- `schema_next/conditional.rs`: supported conditional subset parsing
- `schema_next/errors.rs`: validation error types and reason helpers

## Explicitly deferred

- `crates/assay-evidence/src/lint/packs/checks.rs`
- pack execution/runtime check logic
- loader precedence redesign
- built-in or open-pack content changes
- spec or release-floor changes
