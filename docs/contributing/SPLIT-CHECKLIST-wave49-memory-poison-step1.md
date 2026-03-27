# Wave49 Memory Poison Step1 Checklist (Freeze)

## Scope lock

- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave49-memory-poison.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave49-memory-poison-step1.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave49-memory-poison-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave49-memory-poison-step1.md`
  - `scripts/ci/review-wave49-memory-poison-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-sim/src/attacks/**`
- [ ] No edits under `crates/assay-sim/tests/**`
- [ ] No `assay-core`, `assay-cli`, `assay-evidence`, or report-surface changes

## Frozen contract

- [ ] `PoisonResult`, `PoisonOutcome`, and `run_memory_poison_matrix` are explicitly frozen as the stable facade surface
- [ ] No replay-diff or context-envelope behavior drift is allowed in Step2
- [ ] No snapshot-hash, benign-control, or `AttackStatus` mapping drift is allowed in Step2
- [ ] No result-count, ordering intent, or attack-name drift is allowed in Step2
- [ ] No integration-test expectation drift is allowed in Step2
- [ ] Step2 non-goals explicitly forbid attack redesign, optimization, or test reorganization

## Validation

- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave49-memory-poison-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-sim --all-targets -- -D warnings` passes
- [ ] Pinned memory-poison invariants pass
