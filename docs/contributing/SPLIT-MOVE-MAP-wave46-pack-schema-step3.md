# SPLIT-MOVE-MAP — Wave46 Step3 — `lint/packs/schema.rs` Closure

## Shipped layout

Wave46 Step2 is now the shipped split shape on `main`:
- `crates/assay-evidence/src/lint/packs/schema.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/mod.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/types.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/serde.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/validation.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/conditional.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/errors.rs`

## Ownership freeze

- `crates/assay-evidence/src/lint/packs/schema.rs`
  remains the stable facade for schema exports and inline contract tests.
- `crates/assay-evidence/src/lint/packs/schema_next/types.rs`
  remains the schema type-definition boundary.
- `crates/assay-evidence/src/lint/packs/schema_next/serde.rs`
  remains the pack severity serde helper boundary.
- `crates/assay-evidence/src/lint/packs/schema_next/validation.rs`
  remains the schema validation and pack-name grammar boundary.
- `crates/assay-evidence/src/lint/packs/schema_next/conditional.rs`
  remains the supported conditional subset parsing boundary.
- `crates/assay-evidence/src/lint/packs/schema_next/errors.rs`
  remains the validation error definition boundary.

## Allowed follow-up after closure

- documentation updates only
- reviewer-gate tightening only
- internal visibility tightening only if it requires no code edits in this wave

## Explicitly deferred

- new module cuts
- schema or DSL redesign
- `checks.rs` execution refactors
- pack validation behavior cleanup
- built-in/open parity changes
- validation error or reason-meaning changes
