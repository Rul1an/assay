# Coverage Command Step1 Checklist (Freeze)

Scope lock:
- `docs/contributing/SPLIT-PLAN-wave19-coverage-command.md`
- `docs/contributing/SPLIT-CHECKLIST-coverage-command-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-coverage-command-step1.md`
- `scripts/ci/review-coverage-command-step1.sh`
- no code edits under `crates/assay-cli/src/cli/commands/coverage.rs`
- no code edits under `crates/assay-cli/src/cli/commands/coverage/**`
- no workflow edits

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in coverage command subtree
- hard fail untracked files in coverage command subtree
- `cargo fmt --check`
- `cargo clippy -p assay-cli --all-targets -- -D warnings`
- targeted exact tests:
  - `coverage_contract_generates_valid_report_from_basic_jsonl`
  - `coverage_out_md_writes_json_and_markdown_artifacts`
  - `coverage_declared_tools_file_union_with_flags`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-coverage-command-step1.sh` passes
- Step1 diff contains only the 4 allowlisted files
