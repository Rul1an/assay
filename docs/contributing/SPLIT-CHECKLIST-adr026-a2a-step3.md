# SPLIT CHECKLIST - ADR-026 A2A Step3 (closure)

## Scope
- [ ] Only Step3 closure docs and reviewer gate changed
- [ ] No `.github/workflows/*` changes
- [ ] No runtime behavior changes

## Contracts present
- [ ] `crates/assay-adapter-api/` exists
- [ ] `crates/assay-adapter-a2a/` exists
- [ ] A2A fixtures exist under `scripts/ci/fixtures/adr026/a2a/v0.2/`
- [ ] `scripts/ci/test-adapter-a2a.sh` exists and passes

## Step1/Step2 coverage
- [ ] A2A contract freeze exists (`PLAN-ADR-026-A2A-2026q2.md`)
- [ ] A2A MVP implemented
- [ ] Negative fixtures included
- [ ] Determinism asserted on repeated happy-path conversion

## Reviewer gate
- [ ] `scripts/ci/review-adr026-a2a-step3.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate validates Step1/Step2 artifacts are present
