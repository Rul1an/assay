# Wave6 Step4 inventory: nightly promotion policy freeze

Snapshot baseline (`origin/main` before Step4): `a8917d06`
Working branch head: see `git rev-parse --short HEAD`

Step4 Commit A scope:
- docs + reviewer gates only
- no workflow semantic changes
- no production crate code changes

Target files (Commit A):
- `docs/contributing/SPLIT-INVENTORY-wave6-step4-nightly-promotion.md`
- `docs/contributing/SPLIT-CHECKLIST-wave6-step4-nightly-promotion.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave6-step4-nightly-promotion.md`
- `scripts/ci/review-wave6-step4-ci.sh`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

Baseline anchor workflow (read-only in Commit A):
- `.github/workflows/wave6-nightly-safety.yml`

Promotion policy source (Commit A contract):
- use GitHub Actions runs API as the metric source of truth
- compute over most recent completed `schedule` runs on `main`
- no branch protection edits in Step4

Non-goals (Commit A):
- no nightly workflow edits
- no required-check changes
- no artifact schema implementation yet (specified for Commit B)
