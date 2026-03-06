# SPLIT CHECKLIST - Wave T1 Step A (Test Hardening Freeze)

## Scope Lock

- [ ] Step A is docs + reviewer gate only.
- [ ] No `.github/workflows/*` changes.
- [ ] No production/test logic changes yet in:
  - `crates/assay-core/tests/parity.rs`
  - `crates/assay-registry/src/verify_internal/tests.rs`
  - `crates/assay-cli/tests/contract_exit_codes.rs`

## Inventory Baseline (origin/main)

- [ ] `crates/assay-core/tests/parity.rs` (936 LOC)
- [ ] `crates/assay-registry/src/verify_internal/tests.rs` (901 LOC)
- [ ] `crates/assay-cli/tests/contract_exit_codes.rs` (816 LOC)

## Section Inventory and Planned Split Map

### `crates/assay-core/tests/parity.rs`

- [ ] Core types: `PolicyCheck`, `CheckType`, `CheckInput`, `ToolCall`, `CheckResult`, `Outcome`
- [ ] Engine modules: `batch`, `streaming`, shared evaluator (`shared`)
- [ ] Parity verification path: `compute_result_hash`, `verify_parity`, `ParityResult`
- [ ] Fixtures: `fixtures::*` case builders
- [ ] Test entry module: `tests::*` parity contract tests
- [ ] Planned B1 mapping frozen:
  - `tests/parity/mod.rs` (thin harness + shared exports)
  - `tests/parity/core_types.rs`
  - `tests/parity/engines.rs`
  - `tests/parity/fixtures.rs`
  - `tests/parity/assertions.rs`
  - `tests/parity/parity_contract.rs`

### `crates/assay-registry/src/verify_internal/tests.rs`

- [ ] Digest + digest contract tests (`test_compute_digest_*`, `test_verify_digest_*`)
- [ ] DSSE wire/PAE + payload-type tests (`test_build_pae`, parse-envelope tests)
- [ ] Header-size regression tests
- [ ] DSSE signature vector tests (valid/mismatch/untrusted/wrong type/empty/invalid)
- [ ] Verify-pack fail-closed and canonicalization contracts
- [ ] Planned B2 mapping frozen:
  - `verify_internal/tests/mod.rs`
  - `verify_internal/tests/digest.rs`
  - `verify_internal/tests/dsse.rs`
  - `verify_internal/tests/provenance.rs`
  - `verify_internal/tests/failures.rs`

### `crates/assay-cli/tests/contract_exit_codes.rs`

- [ ] Shared IO/assert helpers (`read_*`, `assert_*`)
- [ ] Run/CI exit-code and reason-code contracts
- [ ] Replay/bundle offline and hermetic contracts
- [ ] Deprecation-deny exit contracts
- [ ] Planned B3 mapping frozen:
  - `tests/exit_codes/mod.rs`
  - `tests/exit_codes/core.rs`
  - `tests/exit_codes/replay.rs`
  - `tests/exit_codes/evidence.rs` (only if coverage exists in B3 inventory)
  - `tests/exit_codes/mcp.rs` (only if coverage exists in B3 inventory)
  - `tests/exit_codes/import.rs` (only if coverage exists in B3 inventory)
  - `tests/exit_codes/coverage.rs` (only if coverage exists in B3 inventory)

## Behavior Freeze

- [ ] Zero semantic drift: assertions, fixtures, and exit-code expectations unchanged.
- [ ] Test naming remains stable wherever mechanically possible.
- [ ] No new dependencies introduced in Step A.

## Reviewer Gate

- [ ] `scripts/ci/review-wave-t1-tests-a.sh` exists.
- [ ] Gate enforces allowlist-only + workflow-ban.
- [ ] Gate runs:
  - `cargo fmt --check`
  - `cargo clippy -p assay-core -p assay-registry -p assay-cli --all-targets -- -D warnings`
  - `cargo test -p assay-core --test parity`
  - `cargo test -p assay-registry verify_internal`
  - `cargo test -p assay-cli --test contract_exit_codes`
- [ ] Gate fails if any target test file changes in Step A.
