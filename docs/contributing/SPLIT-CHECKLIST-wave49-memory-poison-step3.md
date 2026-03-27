# Wave49 Memory Poison Step3 Checklist (Closure)

## Scope lock

- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave49-memory-poison.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave49-memory-poison-step3.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave49-memory-poison-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave49-memory-poison-step3.md`
  - `scripts/ci/review-wave49-memory-poison-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-sim/src/attacks/**`
- [ ] No edits under `crates/assay-sim/tests/**`
- [ ] No new module proposals beyond the shipped `memory_poison_next/*` layout

## Step3 closure contract

- [ ] Step2 is recorded as shipped behind a stable facade
- [ ] The fail-closed replay-basis hashing follow-up is recorded as shipped
- [ ] Step3 is explicitly bounded to micro-cleanup only
- [ ] `memory_poison.rs` remains the stable facade entrypoint
- [ ] `memory_poison_next/*` remains the shipped implementation ownership boundary
- [ ] No replay, context-envelope, status, or matrix-contract drift is proposed in Step3
- [ ] No public `assay-sim` surface expansion is proposed in Step3

## Validation

- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave49-memory-poison-step3.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-sim --all-targets -- -D warnings` passes
- [ ] Pinned memory-poison invariants pass
