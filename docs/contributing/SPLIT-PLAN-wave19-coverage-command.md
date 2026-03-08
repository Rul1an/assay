# Wave19 Plan — `coverage.rs` Command Split

## Goal

Split `crates/assay-cli/src/cli/commands/coverage.rs` into bounded modules with zero behavior change and stable CLI contract.

## Step1 (freeze)

Branch: `codex/wave19-coverage-command-step1-freeze` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave19-coverage-command.md`
- `docs/contributing/SPLIT-CHECKLIST-coverage-command-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-coverage-command-step1.md`
- `scripts/ci/review-coverage-command-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `crates/assay-cli/src/cli/commands/coverage.rs` or `crates/assay-cli/src/cli/commands/coverage/**`
- no workflow edits

Step1 gate:
- allowlist-only diff (the 4 Step1 files)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in coverage command subtree
- hard fail untracked files in coverage command subtree
- `cargo fmt --check`
- `cargo clippy -p assay-cli --all-targets -- -D warnings`
- targeted exact tests:
  - `cargo test -p assay-cli coverage_contract_generates_valid_report_from_basic_jsonl -- --exact`
  - `cargo test -p assay-cli coverage_out_md_writes_json_and_markdown_artifacts -- --exact`
  - `cargo test -p assay-cli coverage_declared_tools_file_union_with_flags -- --exact`

## Step2 (mechanical split preview)

Target layout (preview):
- `crates/assay-cli/src/cli/commands/coverage/mod.rs` (facade + command entry)
- `crates/assay-cli/src/cli/commands/coverage/generate.rs`
- `crates/assay-cli/src/cli/commands/coverage/legacy.rs`
- `crates/assay-cli/src/cli/commands/coverage/io.rs`
- existing helper modules remain bounded (`format_md.rs`, `report.rs`, `schema.rs`)

Step2 principles:
- 1:1 body moves only
- preserve exit-code semantics and output contracts
- keep JSON/markdown schema/format behavior identical
- no product behavior or policy changes

## Step3 (closure)

Docs+gate-only closure slice that re-runs Step2 invariants and keeps allowlist strict.

## Promote

Stacked chain:
- Step1 -> `main`
- Step2 -> Step1
- Step3 -> Step2

Final promote PR to `main` from Step3 once chain is clean.
