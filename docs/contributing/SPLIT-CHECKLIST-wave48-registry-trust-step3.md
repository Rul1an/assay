# Wave48 Registry Trust Step3 Checklist (Closure)

## Scope lock

- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave48-registry-trust.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave48-registry-trust-step3.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave48-registry-trust-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave48-registry-trust-step3.md`
  - `scripts/ci/review-wave48-registry-trust-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-registry/src/**`
- [ ] No edits under `crates/assay-registry/tests/**`
- [ ] No new module proposals beyond the shipped `trust_next/*` layout

## Step3 closure contract

- [ ] Step2 is recorded as shipped behind a stable facade
- [ ] Step3 is explicitly bounded to micro-cleanup only
- [ ] `trust.rs` remains the stable facade entrypoint
- [ ] `trust_next/*` remains the split implementation ownership boundary
- [ ] No trust, resolver, validation, cache, or verification coupling drift is allowed in Step3
- [ ] No public registry trust surface expansion is proposed in Step3

## Validation

- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave48-registry-trust-step3.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-registry --all-targets -- -D warnings` passes
- [ ] Pinned trust/resolver invariants pass
