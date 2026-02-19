# ADR-025 Step4C Closure Checklist

## Scope guardrails
- [ ] Docs/scripts only in this slice
- [ ] No `.github/workflows/*` file changes
- [ ] No PR required-check behavior changes

## Step4 closure artifacts
- [ ] Runbook exists: `docs/ops/ADR-025-SOAK-ENFORCEMENT-RUNBOOK.md`
- [ ] Checklist exists: `docs/contributing/SPLIT-CHECKLIST-adr025-step4-c-closure.md`
- [ ] Review pack exists: `docs/contributing/SPLIT-REVIEW-PACK-adr025-step4-c-closure.md`
- [ ] Reviewer script exists: `scripts/ci/review-adr025-i1-step4-c.sh`

## Invariant checks documented
- [ ] Release workflow contains ADR-025 enforcement step
- [ ] Enforcement references `schemas/soak_readiness_policy_v1.json`
- [ ] Enforcement consumes `adr025-nightly-readiness` artifact
- [ ] Exit contract 0/1/2 documented in runbook and plan

## Planning/docs sync
- [ ] Step4 status updated in PLAN
- [ ] Step4 status updated in ROADMAP
