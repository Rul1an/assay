# Wave6 Step4 inventory: nightly promotion policy freeze

Snapshot baseline (`origin/main` before Step4): `a8917d06`
Working branch head: see `git rev-parse --short HEAD`

Step4 Commit B scope:
- nightly workflow instrumentation + docs/reviewer gates
- no production crate code changes
- no required-check/branch-protection changes

Target files (Step4):
- `.github/workflows/wave6-nightly-safety.yml`
- `.github/workflows/wave6-nightly-readiness.yml`
- `docs/contributing/SPLIT-INVENTORY-wave6-step4-nightly-promotion.md`
- `docs/contributing/SPLIT-CHECKLIST-wave6-step4-nightly-promotion.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave6-step4-nightly-promotion.md`
- `scripts/ci/review-wave6-step4-ci.sh`
- `scripts/ci/wave6-nightly-readiness-report.sh`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

Instrumentation choice:
- Option A: centralized API aggregator in `nightly-summary` job writes one `nightly_status.json`.

Readiness reporting choice (Commit C):
- separate informative workflow (`wave6-nightly-readiness.yml`)
- trigger: `schedule` + `workflow_dispatch` only (no `pull_request`)
- output artifact:
  - name: `nightly-readiness-report`
  - files: `nightly_readiness_report.json`, `nightly_readiness_report.md`
  - retention: 14 days

Promotion policy source:
- use GitHub Actions runs API as the metric source of truth
- compute over most recent completed `schedule` runs on `main`
- no branch protection edits in Step4

Artifact contract:
- name: `nightly-status`
- file: `nightly_status.json`
- retention: 14 days

Non-goals:
- no required-check changes
- no branch-protection edits
- no pull_request trigger on readiness workflow
