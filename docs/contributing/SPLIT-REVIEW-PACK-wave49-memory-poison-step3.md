# Wave49 Memory Poison Step3 Review Pack (Closure)

## Intent

Close the shipped `memory_poison.rs` split with docs/gates only and explicitly forbid any new
memory-poison semantic or surface drift in the closure slice.

## Scope

- `docs/contributing/SPLIT-PLAN-wave49-memory-poison.md`
- `docs/contributing/SPLIT-CHECKLIST-wave49-memory-poison-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave49-memory-poison-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave49-memory-poison-step3.md`
- `scripts/ci/review-wave49-memory-poison-step3.sh`

## Non-goals

- no workflow changes
- no edits under `crates/assay-sim/src/attacks/**`
- no edits under `crates/assay-sim/tests/**`
- no new vectors or controls
- no matrix redesign
- no replay/context/status/result drift
- no `assay-core`, `assay-cli`, or `assay-evidence` coupling drift

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave49-memory-poison-step3.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-sim --all-targets -- -D warnings
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::vector1_activates_under_condition_a' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::vector3_activates_under_condition_a' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::controls_produce_no_false_positives' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::full_matrix_runs_without_panic' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::condition_b_blocks_v1_and_v2' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::condition_c_blocks_v3' -- --exact
cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::overarching_invariant_controls_never_misclassify' -- --exact
cargo test -q -p assay-sim --test memory_poison_invariant overarching_invariant_no_silent_downgrades_in_controls -- --exact
cargo test -q -p assay-sim --test memory_poison_invariant attack_vectors_activate_under_condition_a -- --exact
cargo test -q -p assay-sim --test memory_poison_invariant condition_b_blocks_replay_vectors -- --exact
cargo test -q -p assay-sim --test memory_poison_invariant condition_c_blocks_context_envelope -- --exact
cargo test -q -p assay-sim --test memory_poison_invariant full_matrix_structure -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to the Step3 allowlist.
2. Confirm the plan records both `#973` and the fail-closed follow-up `#975` as shipped on `main`.
3. Confirm the move-map reflects the shipped `memory_poison_next/*` ownership boundary and proposes no new module cuts.
4. Confirm `crates/assay-sim/src/attacks/**` and `crates/assay-sim/tests/**` stayed untouched.
5. Confirm the reviewer script reruns the pinned memory-poison invariants without adding new scope.
