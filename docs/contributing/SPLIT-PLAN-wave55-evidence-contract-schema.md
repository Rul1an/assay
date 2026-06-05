# Wave55 Evidence Contract Split Plan

## Scope

Wave55 starts the post-Wave54 evidence-contract refactor line with narrow, contract-preserving
targets:

- `crates/assay-cli/src/cli/commands/evidence/schema.rs`
- `crates/assay-cli/src/cli/commands/evidence/pydantic_case_result.rs`

These files own serialized CLI contracts: schema listing/validation and external importer receipt
reduction. The wave preserves behavior and output shape while making each contract easier to review.

## Step 1 Target

Split `schema.rs` behind its existing public command facade:

- keep `SchemaArgs`, `SchemaCmd`, subcommand args, and `cmd_schema` in `schema.rs`
- move embedded schema descriptors and lookup to `schema/registry.rs`
- move serializable report and validation error types to `schema/reports.rs`
- move JSON/JSONL validation to `schema/validate.rs`
- move text/JSON rendering to `schema/write.rs`

## Step 2 Target

Split `pydantic_case_result.rs` behind its existing public importer facade:

- keep `PydanticCaseResultArgs` and `cmd_pydantic_case_result` in `pydantic_case_result.rs`
- move event/schema constants to `pydantic_case_result/constants.rs`
- move JSONL event construction to `pydantic_case_result/events.rs`
- move receipt payload reduction to `pydantic_case_result/reduce.rs`
- move source artifact/import-time helpers to `pydantic_case_result/source.rs`
- move bounded validation helpers to `pydantic_case_result/validate.rs`
- move existing importer unit tests to `pydantic_case_result/tests.rs`

## Non-Goals

- No schema ID, alias, path, family, status, or Trust Basis claim changes.
- No importer behavior changes.
- No receipt JSON Schema file changes.
- No docs/reference content changes.
- No `.github/workflows/**` edits.
- No CycloneDX or Mastra importer moves in Step 2.

## Review Rule

Review each step as a mechanical split against `origin/main`. The contract tests in
`crates/assay-cli/tests/receipt_schema_registry_test*` and `crates/assay-cli/tests/evidence_test*`
are the behavior freeze.
