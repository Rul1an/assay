# SPLIT REVIEW PACK - Wave55 Step1 - Evidence Schema Facade Split

## Scope

Step1 mechanically splits the `assay evidence schema` CLI implementation behind the stable
`schema.rs` facade.

## Files

- `docs/contributing/SPLIT-PLAN-wave55-evidence-contract-schema.md`
- `docs/contributing/SPLIT-CHECKLIST-wave55-evidence-contract-schema-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave55-evidence-contract-schema-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave55-evidence-contract-schema-step1.md`
- `scripts/ci/review-wave55-evidence-contract-schema-step1.sh`
- `crates/assay-cli/src/cli/commands/evidence/schema.rs`
- `crates/assay-cli/src/cli/commands/evidence/schema/registry.rs`
- `crates/assay-cli/src/cli/commands/evidence/schema/reports.rs`
- `crates/assay-cli/src/cli/commands/evidence/schema/validate.rs`
- `crates/assay-cli/src/cli/commands/evidence/schema/write.rs`

## Verification

Run:

```bash
BASE_REF=origin/main bash scripts/ci/review-wave55-evidence-contract-schema-step1.sh
```

The gate runs:

```bash
cargo fmt --check
cargo check -p assay-cli
cargo test -q -p assay-cli --test receipt_schema_registry_test
cargo clippy -p assay-cli --all-targets -- -D warnings
git diff --check
```

## Reviewer Focus

- Confirm `schema.rs` still owns the CLI args and `cmd_schema` dispatch.
- Confirm descriptor names, aliases, schema IDs, paths, status, family, and Trust Basis claim fields
  are mechanically preserved in `schema/registry.rs`.
- Confirm JSON/JSONL validation preserves parse/config/schema exit-code behavior.
- Confirm text and JSON report output contracts remain covered by `receipt_schema_registry_test`.
- Confirm importer command behavior is untouched.

## LOC Deltas

| File | Before LOC | After LOC |
| --- | ---: | ---: |
| `crates/assay-cli/src/cli/commands/evidence/schema.rs` | 627 | 139 |

Moved modules:

| File | LOC |
| --- | ---: |
| `crates/assay-cli/src/cli/commands/evidence/schema/registry.rs` | 256 |
| `crates/assay-cli/src/cli/commands/evidence/schema/reports.rs` | 87 |
| `crates/assay-cli/src/cli/commands/evidence/schema/validate.rs` | 86 |
| `crates/assay-cli/src/cli/commands/evidence/schema/write.rs` | 76 |

## Next Candidates

After this PR lands, the next evidence-contract candidates are the importer model files:

- `pydantic_case_result.rs`
- `cyclonedx_mlbom_model.rs`
- `mastra_score_event.rs`

Each should get its own contract-preserving split, not a combined broad cleanup.
