# SPLIT CHECKLIST - Wave T1 B2 (verify_internal tests Mechanical Split)

## Scope Lock

- [ ] Mechanical move only for `crates/assay-registry/src/verify_internal/tests.rs`.
- [ ] No `.github/workflows/*` changes.
- [ ] No test assertion semantic changes.

## File Layout

- [ ] Legacy single file replaced by module directory:
  - `crates/assay-registry/src/verify_internal/tests/mod.rs`
  - `crates/assay-registry/src/verify_internal/tests/digest.rs`
  - `crates/assay-registry/src/verify_internal/tests/dsse.rs`
  - `crates/assay-registry/src/verify_internal/tests/provenance.rs`
  - `crates/assay-registry/src/verify_internal/tests/failures.rs`

## Behavior Freeze

- [ ] All original `verify_internal` test functions preserved.
- [ ] Existing helper wrapper behavior unchanged (`canonicalize_for_dsse`, `parse_dsse_envelope`, `build_pae`, signature helper paths).
- [ ] `cargo test -p assay-registry verify_internal` remains green.

## Boundary / Single-Source

- [ ] Shared helper constructors stay centralized in `tests/mod.rs`.
- [ ] DSSE vector tests live in `tests/dsse.rs`.
- [ ] Canonicalization/provenance contracts live in `tests/provenance.rs`.
- [ ] Fail-closed matrix/reason-stability contracts live in `tests/failures.rs`.

## Reviewer Gate

- [ ] `scripts/ci/review-wave-t1-b2-verify-internal.sh` exists.
- [ ] Gate enforces allowlist-only + workflow-ban.
- [ ] Gate executes:
  - `cargo fmt --check`
  - `cargo clippy -p assay-registry --tests -- -D warnings`
  - `cargo test -p assay-registry verify_internal`
- [ ] Gate enforces no-increase drift counters on split surface.
