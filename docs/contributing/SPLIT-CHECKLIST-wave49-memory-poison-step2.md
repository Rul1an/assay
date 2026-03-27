# Wave49 Memory Poison Step2 Checklist (Mechanical Split)

## Scope lock

- [ ] Only these files changed:
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
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-sim/tests/**`
- [ ] No `assay-core`, `assay-cli`, or `assay-evidence` scope leak

## Mechanical split contract

- [ ] `memory_poison.rs` remains the stable facade entrypoint for public vectors, controls, and `run_memory_poison_matrix`
- [ ] `memory_poison_next/*` carries the moved implementation bodies
- [ ] No replay-diff or context-envelope drift
- [ ] No snapshot-hash or benign-control drift
- [ ] No `PoisonOutcome` / `AttackStatus` mapping drift
- [ ] No matrix count or attack-name drift
- [ ] Inline tests remain in `memory_poison.rs`

## Validation

- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave49-memory-poison-step2.sh` passes
- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy -p assay-sim --all-targets -- -D warnings` passes
- [ ] Pinned memory-poison invariants pass
