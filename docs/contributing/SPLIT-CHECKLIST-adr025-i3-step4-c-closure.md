# SPLIT CHECKLIST — ADR-025 I3 Step4C (closure slice)

## Scope (hard)
- [ ] Only Step4C allowlist files changed
- [ ] No `.github/workflows/*` changes
- [ ] No runtime behavior changes (docs + reviewer gate only)

## Contracts (must match current main)
- [ ] Policy exists: `schemas/otel_release_policy_v1.json`
- [ ] Release script exists: `scripts/ci/adr025-otel-release.sh`
- [ ] Step4B reviewer gate exists: `scripts/ci/review-adr025-i3-step4-b.sh`
- [ ] Release wiring references OTel release script in `.github/workflows/release.yml`

## Mode contract
- [ ] Modes documented: `off|attach|warn|enforce`
- [ ] Default documented: `attach`
- [ ] Enforce semantics documented as `contract_only`
- [ ] Exit contract documented: `0/1/2` with meanings

## Runbook completeness
- [ ] Missing artifact flow
- [ ] Schema/contract mismatch flow
- [ ] Policy-fail reserved semantics clarified
- [ ] Break-glass override rules + audit trail

## Reviewer gates
- [ ] `scripts/ci/review-adr025-i3-step4-c.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate verifies invariants on policy/script/release wiring
