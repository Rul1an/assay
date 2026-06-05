# Wave55 Evidence Contract Schema Split Plan

## Scope

Wave55 starts the post-Wave54 evidence-contract refactor line with one narrow target:

- `crates/assay-cli/src/cli/commands/evidence/schema.rs`

This file owns the `assay evidence schema` CLI contract for listing, showing, and validating
receipt/input schemas. Because the surface is a serialized CLI contract, this wave preserves behavior
and output shape before considering any importer-model splits.

## Step 1 Target

Split `schema.rs` behind its existing public command facade:

- keep `SchemaArgs`, `SchemaCmd`, subcommand args, and `cmd_schema` in `schema.rs`
- move embedded schema descriptors and lookup to `schema/registry.rs`
- move serializable report and validation error types to `schema/reports.rs`
- move JSON/JSONL validation to `schema/validate.rs`
- move text/JSON rendering to `schema/write.rs`

## Non-Goals

- No schema ID, alias, path, family, status, or Trust Basis claim changes.
- No importer behavior changes.
- No receipt JSON Schema file changes.
- No docs/reference content changes.
- No `.github/workflows/**` edits.

## Review Rule

Review this as a mechanical split against `origin/main`. The contract tests in
`crates/assay-cli/tests/receipt_schema_registry_test*` are the behavior freeze.
