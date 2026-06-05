# SPLIT MOVE MAP - Wave55 Step1 - Evidence Schema Facade Split

## Base

- base: `origin/main`
- head: `codex/wave55-evidence-contract-split`

## Mechanical Movement

Facade:

- `crates/assay-cli/src/cli/commands/evidence/schema.rs`

New internal modules:

- `crates/assay-cli/src/cli/commands/evidence/schema/registry.rs`
- `crates/assay-cli/src/cli/commands/evidence/schema/reports.rs`
- `crates/assay-cli/src/cli/commands/evidence/schema/validate.rs`
- `crates/assay-cli/src/cli/commands/evidence/schema/write.rs`

## Explicit Non-Movement

- No edits to `crates/assay-cli/src/cli/commands/evidence/*_result.rs`.
- No edits to `crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model.rs`.
- No edits to `crates/assay-cli/src/cli/commands/evidence/mastra_score_event.rs`.
- No edits to `crates/assay-cli/receipt-schemas/**`.
- No edits to `docs/reference/receipt-schemas/**`.
- No edits to `.github/workflows/**`.

## LOC Snapshot

| Area | Before LOC | After LOC |
| --- | ---: | ---: |
| `schema.rs` facade | 627 | 139 |

Moved code now lives in:

| File | LOC |
| --- | ---: |
| `schema/registry.rs` | 256 |
| `schema/reports.rs` | 87 |
| `schema/validate.rs` | 86 |
| `schema/write.rs` | 76 |
