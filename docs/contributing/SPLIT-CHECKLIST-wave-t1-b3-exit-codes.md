# SPLIT CHECKLIST - Wave T1 B3 (contract_exit_codes Mechanical Split)

## Scope Lock

- [ ] Mechanical move only for `crates/assay-cli/tests/contract_exit_codes.rs`.
- [ ] No `.github/workflows/*` changes.
- [ ] Exit-code expectations and reason-code assertions unchanged.

## File Layout

- [ ] `crates/assay-cli/tests/contract_exit_codes.rs` is helper/facade + module wiring.
- [ ] New split modules:
  - `crates/assay-cli/tests/exit_codes/core.rs`
  - `crates/assay-cli/tests/exit_codes/replay.rs`

## Behavior Freeze

- [ ] All 13 contract tests preserved.
- [ ] Run/CI/replay/deprecation contract semantics unchanged.
- [ ] `test_status_map` helper behavior unchanged.

## Boundary / Single-Source

- [ ] JSON/run-summary helper functions remain single-source in root file.
- [ ] Core exit-code contract tests grouped in `exit_codes/core.rs`.
- [ ] Replay/offline contract tests grouped in `exit_codes/replay.rs`.

## Reviewer Gate

- [ ] `scripts/ci/review-wave-t1-b3-exit-codes.sh` exists.
- [ ] Gate enforces allowlist-only + workflow-ban.
- [ ] Gate executes:
  - `cargo fmt --check`
  - `cargo clippy -p assay-cli --test contract_exit_codes -- -D warnings`
  - `cargo test -p assay-cli --test contract_exit_codes`
- [ ] Gate enforces no-increase drift counters on split surface.
