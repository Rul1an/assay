# SPLIT REVIEW PACK - T-R1 Decision Emit Invariant Step3

## Summary
This closure slice finishes T-R1 without changing the `decision_emit_invariant` target itself.

The final shape remains:
- one integration target under `tests/decision_emit_invariant/main.rs`
- shared helpers in `fixtures.rs`
- scenario families split across dedicated modules
- no production changes and no new test-target fragmentation

## Review focus
- confirm only docs/gates changed
- confirm the final target shape is documented consistently
- confirm no edits landed under `crates/assay-core/tests/decision_emit_invariant/**`
- confirm no edits landed under `crates/assay-core/src/**`

## Validation
- `BASE_REF=origin/main bash scripts/ci/review-tr1-decision-emit-invariant-step3.sh`
- `cargo fmt --all --check`
- `cargo clippy -q -p assay-core --all-targets -- -D warnings`
