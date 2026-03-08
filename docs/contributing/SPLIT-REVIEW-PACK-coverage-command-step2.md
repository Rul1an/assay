# Coverage Command Step2 Review Pack (Mechanical)

## Intent

Mechanically split `crates/assay-cli/src/cli/commands/coverage.rs` into bounded modules while preserving behavior and contracts.

## Scope

- `crates/assay-cli/src/cli/commands/coverage.rs` (removed; replaced by directory module facade)
- `crates/assay-cli/src/cli/commands/coverage/mod.rs`
- `crates/assay-cli/src/cli/commands/coverage/generate.rs`
- `crates/assay-cli/src/cli/commands/coverage/legacy.rs`
- `crates/assay-cli/src/cli/commands/coverage/io.rs`
- existing bounded helper modules in `coverage/`
- Step2 docs + `scripts/ci/review-coverage-command-step2.sh`

## Non-goals

- no workflow changes
- no CLI/exit-code behavior changes
- no `mcp wrap --coverage-out` changes
- no semantic cleanup beyond mechanical relocation

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-coverage-command-step2.sh
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

1. Confirm diff is allowlisted and workflow-free.
2. Confirm `coverage/mod.rs` is thin facade with module wiring + wrappers only.
3. Confirm generator logic is in `generate.rs`, legacy logic in `legacy.rs`, IO helpers in `io.rs`.
4. Run reviewer script and expect PASS.
