# Wave T1 B3 Review Pack - contract_exit_codes Mechanical Split

## Intent

Split a large integration contract test file into core/replay modules while keeping machine-contract behavior frozen.

## Scope

- `crates/assay-cli/tests/contract_exit_codes.rs`
- `crates/assay-cli/tests/exit_codes/*`
- `docs/contributing/SPLIT-CHECKLIST-wave-t1-b3-exit-codes.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-t1-b3-exit-codes.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-t1-b3-exit-codes.md`
- `scripts/ci/review-wave-t1-b3-exit-codes.sh`

## Non-goals

- No reason-code contract changes.
- No exit-code mapping changes.
- No workflow changes.

## Validation Command

```bash
BASE_REF=<previous-step-commit> bash scripts/ci/review-wave-t1-b3-exit-codes.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-cli --test contract_exit_codes -- -D warnings
cargo test -p assay-cli --test contract_exit_codes
```

## Reviewer 60s Scan

1. Confirm helper/facade in root and tests moved to `exit_codes/{core,replay}.rs`.
2. Confirm all 13 test anchors still exist.
3. Confirm no new workflow or off-scope files changed.
4. Run reviewer script and verify PASS.
