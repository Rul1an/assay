# Coverage Command Step2 Checklist (Mechanical)

Scope lock:
- `crates/assay-cli/src/cli/commands/coverage.rs` (deleted as part of move to `coverage/mod.rs`)
- `crates/assay-cli/src/cli/commands/coverage/mod.rs`
- `crates/assay-cli/src/cli/commands/coverage/generate.rs`
- `crates/assay-cli/src/cli/commands/coverage/legacy.rs`
- `crates/assay-cli/src/cli/commands/coverage/io.rs`
- existing bounded helpers under `crates/assay-cli/src/cli/commands/coverage/` (`format_md.rs`, `report.rs`, `schema.rs`)
- `docs/contributing/SPLIT-CHECKLIST-coverage-command-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-coverage-command-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-coverage-command-step2.md`
- `scripts/ci/review-coverage-command-step2.sh`
- no workflow edits

## Mechanical invariants

- `coverage/mod.rs` is facade-only (dispatch + wrappers + module wiring).
- Generator-mode logic lives in `coverage/generate.rs`.
- Legacy analyzer/baseline path lives in `coverage/legacy.rs`.
- File-write helpers live in `coverage/io.rs`.
- No behavior change in CLI contract, output contract, or exit-code mapping.

## Gate expectations

- allowlist-only diff vs `BASE_REF`
- workflow-ban (`.github/workflows/*`)
- no untracked files under `crates/assay-cli/src/cli/commands/coverage/**`
- `cargo fmt --check`
- `cargo clippy -p assay-cli --all-targets -- -D warnings`
- exact targeted tests:
  - `coverage_contract_generates_valid_report_from_basic_jsonl`
  - `coverage_out_md_writes_json_and_markdown_artifacts`
  - `coverage_declared_tools_file_union_with_flags`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-coverage-command-step2.sh` passes
- Step2 diff contains only allowlisted files
- facade/boundary invariants pass in the reviewer script
