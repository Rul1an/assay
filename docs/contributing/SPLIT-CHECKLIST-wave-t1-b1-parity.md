# SPLIT CHECKLIST - Wave T1 B1 (parity.rs Mechanical Split)

## Scope Lock

- [ ] Mechanical move only for `crates/assay-core/tests/parity.rs`.
- [ ] No `.github/workflows/*` changes.
- [ ] No new test scenarios or assertion semantics.

## File Layout

- [ ] `crates/assay-core/tests/parity.rs` is a thin facade.
- [ ] New split modules exist under `crates/assay-core/tests/parity/`:
  - `core_types.rs`
  - `shared.rs`
  - `batch.rs`
  - `streaming.rs`
  - `assertions.rs`
  - `fixtures.rs`
  - `parity_contract.rs`

## Behavior Freeze

- [ ] Parity contract test set unchanged:
  - `test_all_parity`
  - `test_args_valid_parity`
  - `test_sequence_parity`
  - `test_blocklist_parity`
  - `test_hash_determinism`
- [ ] `verify_parity` outcome parity logic unchanged.
- [ ] `compute_result_hash` behavior unchanged.

## Boundary / Single-Source

- [ ] `compute_result_hash` is defined once (`assertions.rs`).
- [ ] `verify_parity` is defined once (`assertions.rs`).
- [ ] Shared policy logic remains in one module (`shared.rs`) and is reused by `batch.rs` and `streaming.rs`.

## Reviewer Gate

- [ ] `scripts/ci/review-wave-t1-b1-parity.sh` exists.
- [ ] Gate enforces allowlist-only + workflow-ban.
- [ ] Gate executes:
  - `cargo fmt --check`
  - `cargo clippy -p assay-core --tests -- -D warnings`
  - `cargo test -p assay-core --test parity`
- [ ] Gate enforces no-increase drift counters (panic/unwrap/unsafe/print) for split surface.
