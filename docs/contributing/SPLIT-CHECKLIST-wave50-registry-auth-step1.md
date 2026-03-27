# Wave50 Registry Auth Step1 Checklist

- [ ] Step1 is docs+gate only.
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave50-registry-auth.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave50-registry-auth-step1.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave50-registry-auth-step1.md`
  - `scripts/ci/review-wave50-registry-auth-step1.sh`
- [ ] No `.github/workflows/*` changes.
- [ ] No edits under `crates/assay-registry/src/**`.
- [ ] No edits under `crates/assay-registry/tests/**`.
- [ ] Step1 gate pins static/env auth semantics, OIDC exchange/cache/retry semantics, and downstream registry-client auth-header behavior.
- [ ] Validation run:
  - `BASE_REF=origin/main bash scripts/ci/review-wave50-registry-auth-step1.sh`
