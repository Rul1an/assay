# Wave48 Registry Trust Step1 Checklist (Freeze)

## Scope lock

- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave48-registry-trust.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave48-registry-trust-step1.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave48-registry-trust-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave48-registry-trust-step1.md`
  - `scripts/ci/review-wave48-registry-trust-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-registry/src/**`
- [ ] No edits under `crates/assay-registry/tests/**`
- [ ] No verify / resolver / client / CLI / evidence changes

## Frozen contract

- [ ] `TrustStore` and its public entrypoints are explicitly frozen as the stable facade
- [ ] No pinned-root loading drift is allowed in Step2
- [ ] No key-id / SPKI validation drift is allowed in Step2
- [ ] No revoked/expired-key handling drift is allowed in Step2
- [ ] No cache/refresh drift is allowed in Step2
- [ ] No resolver / verification coupling drift is allowed in Step2
- [ ] Step2 non-goals explicitly forbid trust-policy redesign, optimization, or test reorganization

## Validation

- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave48-registry-trust-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-registry --all-targets -- -D warnings` passes
- [ ] Pinned trust/resolver invariants pass
