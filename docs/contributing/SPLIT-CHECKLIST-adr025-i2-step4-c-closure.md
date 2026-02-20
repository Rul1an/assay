# SPLIT CHECKLIST — ADR-025 I2 Step4C (closure slice)

## Scope (hard)
- [ ] Only Step4C allowlist files changed
- [ ] No `.github/workflows/*` changes
- [ ] No runtime behavior changes (docs + reviewer gate only)

## Contracts (must match current main)
- [ ] Policy exists: `schemas/closure_release_policy_v1.json`
- [ ] Release script exists: `scripts/ci/adr025-closure-release.sh`
- [ ] Step4B reviewer gate exists: `scripts/ci/review-adr025-i2-step4-b.sh`
- [ ] Release wiring references release script in `.github/workflows/release.yml`

## Mode contract
- [ ] Modes documented: `off|attach|warn|enforce`
- [ ] Default documented: `attach`
- [ ] Exit contract documented: `0/1/2` with meanings

## Runbook completeness
- [ ] Missing artifact flow
- [ ] Classifier/schema mismatch flow
- [ ] Score below threshold flow
- [ ] Break-glass override rules + audit trail

## Reviewer gates
- [ ] `scripts/ci/review-adr025-i2-step4-c.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate verifies invariants on main assets (policy/script/release wiring)
