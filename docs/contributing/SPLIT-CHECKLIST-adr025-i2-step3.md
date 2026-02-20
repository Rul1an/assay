# ADR-025 I2 Step3 Checklist (closure rollout informational)

## Scope
- [ ] Workflow exists: `.github/workflows/adr025-nightly-closure.yml`
- [ ] Reviewer gate exists: `scripts/ci/review-adr025-i2-step3.sh`
- [ ] Review pack exists: `docs/contributing/SPLIT-REVIEW-PACK-adr025-i2-step3.md`

## Trigger/permissions contracts
- [ ] Triggers are `schedule` + `workflow_dispatch` only
- [ ] No `pull_request`/`pull_request_target`
- [ ] Job-level `continue-on-error: true`
- [ ] Actions are SHA-pinned
- [ ] Minimal permissions (`contents: read`, `actions: write`)

## Artifact contract
- [ ] Upload artifact `adr025-closure-report`
- [ ] Contains `closure_report_v1.json` and `closure_report_v1.md`
- [ ] Retention is 14 days

## Safety
- [ ] Informational-only lane (no PR required-check impact)
- [ ] No changes outside Step3 allowlist
