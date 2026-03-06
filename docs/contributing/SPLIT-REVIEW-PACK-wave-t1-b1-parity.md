# Wave T1 B1 Review Pack - parity.rs Mechanical Split

## Intent

Reduce review surface for parity tests by splitting one large file into cohesive modules while keeping behavior frozen.

## Scope

- `crates/assay-core/tests/parity.rs`
- `crates/assay-core/tests/parity/*`
- `docs/contributing/SPLIT-CHECKLIST-wave-t1-b1-parity.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-t1-b1-parity.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-t1-b1-parity.md`
- `scripts/ci/review-wave-t1-b1-parity.sh`

## Non-goals

- No new parity scenarios.
- No expected-outcome changes.
- No output-text contract changes.

## Validation Command

```bash
BASE_REF=origin/main bash scripts/ci/review-wave-t1-b1-parity.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --tests -- -D warnings
cargo test -p assay-core --test parity
```

## Reviewer 60s Scan

1. Confirm facade + split module layout only.
2. Confirm `compute_result_hash`/`verify_parity` single-source in `assertions.rs`.
3. Confirm test function names are preserved.
4. Run reviewer script and check PASS.
