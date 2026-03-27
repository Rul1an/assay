# Wave49 Memory Poison Step2 Review Pack (Mechanical Split)

## Intent

Execute the `memory_poison.rs` split as a mechanical relocation behind a stable facade and forbid
replay/context/result drift.

## Scope

- `crates/assay-sim/src/attacks/memory_poison.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/mod.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/basis.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/vectors.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/controls.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/conditions.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/matrix.rs`
- `docs/contributing/SPLIT-PLAN-wave49-memory-poison.md`
- `docs/contributing/SPLIT-CHECKLIST-wave49-memory-poison-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave49-memory-poison-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave49-memory-poison-step2.md`
- `scripts/ci/review-wave49-memory-poison-step2.sh`

## Non-goals

- no workflow changes
- no changes under `crates/assay-sim/tests/**`
- no new vectors or controls
- no matrix redesign
- no replay/context/status/result drift
- no `assay-core`, `assay-cli`, or `assay-evidence` coupling drift

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave49-memory-poison-step2.sh
```

Gate includes:

```bash
cargo fmt --all --check
cargo clippy -p assay-sim --all-targets -- -D warnings
cargo check -q -p assay-sim
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

1. Confirm the diff is limited to the Step2 allowlist.
2. Confirm `memory_poison.rs` is now a thin facade plus existing inline tests.
3. Confirm `crates/assay-sim/tests/**` stayed untouched.
4. Confirm the move-map matches the actual `memory_poison_next/*` split.
5. Confirm the reviewer script reruns both inline and integration memory-poison invariants.
