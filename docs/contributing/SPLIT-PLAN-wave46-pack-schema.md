# Wave46 Plan — `lint/packs/schema.rs` Kernel Split

## Goal

Split `crates/assay-evidence/src/lint/packs/schema.rs` behind a stable facade so the pack-schema contract can be reviewed in smaller, responsibility-based modules without changing validation behavior.

Current hotspot baseline on `origin/main @ 23685893`:
- `crates/assay-evidence/src/lint/packs/schema.rs`: `844` LOC before Step2, `245` LOC after Step2
- `crates/assay-evidence/src/lint/packs/checks.rs`: `785` LOC
- `crates/assay-evidence/tests/pack_engine_conditional_test.rs`: conditional contract companion
- `crates/assay-evidence/tests/a2a_discovery_card_followup_pack.rs`: built-in/open parity companion
- `crates/assay-evidence/tests/mcp_signal_followup_pack.rs`: built-in/open parity companion

## Status

- Wave46 Step1 merged on `main` via `#963`.
- Wave46 Step2 shipped on `main` via `#964`.
- Step3 is the closure slice that records the shipped split and forbids follow-on redesign drift in this wave.

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

## Shipped Step2 layout

- `crates/assay-evidence/src/lint/packs/schema.rs` keeps `schema.rs` as the stable facade entrypoint
- `crates/assay-evidence/src/lint/packs/schema_next/mod.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/types.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/serde.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/validation.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/conditional.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/errors.rs`

## Step3 constraints

- docs+gates only
- no edits under `crates/assay-evidence/src/lint/packs/**`
- no edits under `crates/assay-evidence/tests/**`
- no edits under `packs/open/**`
- no workflow edits
- no new module cuts
- no behavior cleanup beyond internal follow-up notes

## Reviewer notes

Primary failure modes:
- moving validation semantics while claiming a mechanical split
- relaxing `json_path_exists.value_equals` or conditional-shape rules during decomposition
- changing loader-visible pack acceptance behavior via schema-only churn
- mixing `checks.rs` execution logic into the `schema.rs` wave

## Non-goals

- No edits to `crates/assay-evidence/src/lint/packs/checks.rs`.
- No edits to `crates/assay-evidence/tests/**`.
- No changes under `packs/open/**` or built-in pack payloads.
- No error-message cleanup, schema redesign, or check DSL expansion.
