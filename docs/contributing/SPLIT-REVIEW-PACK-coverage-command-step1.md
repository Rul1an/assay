# Coverage Command Step1 Review Pack (Freeze)

## Intent

Freeze Wave19 scope for `crates/assay-cli/src/cli/commands/coverage.rs` before any mechanical moves.

## Scope

- `docs/contributing/SPLIT-PLAN-wave19-coverage-command.md`
- `docs/contributing/SPLIT-CHECKLIST-coverage-command-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-coverage-command-step1.md`
- `scripts/ci/review-coverage-command-step1.sh`

## Non-goals

- no changes under coverage command code
- no workflow changes
- no behavior/API changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-coverage-command-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli coverage_contract_generates_valid_report_from_basic_jsonl -- --exact
cargo test -p assay-cli coverage_out_md_writes_json_and_markdown_artifacts -- --exact
cargo test -p assay-cli coverage_declared_tools_file_union_with_flags -- --exact
```

## Reviewer 60s scan

1. Confirm diff is only the 4 Step1 files.
2. Confirm workflow-ban and coverage subtree bans exist in the script.
3. Confirm targeted tests are pinned with `--exact`.
4. Run reviewer script and expect PASS.
