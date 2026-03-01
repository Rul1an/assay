# SPLIT CHECKLIST - ADR-026 UCP Step3 (closure)

## Scope
- [ ] Only Step3 closure docs and reviewer gate changed
- [ ] No `.github/workflows/*` changes
- [ ] No runtime behavior changes

## Contracts present
- [ ] `crates/assay-adapter-api/` exists
- [ ] `crates/assay-adapter-ucp/` exists
- [ ] UCP fixtures exist under `scripts/ci/fixtures/adr026/ucp/v2026-01-23/`
- [ ] `scripts/ci/test-adapter-ucp.sh` exists and passes

## Step1/Step2 coverage
- [ ] UCP contract freeze exists (`PLAN-ADR-026-UCP-2026q2.md`)
- [ ] UCP MVP implemented for the frozen event families
- [ ] Negative fixtures included
- [ ] Determinism asserted on repeated happy-path conversion and key-order independence
- [ ] Parser caps enforced for payload size, JSON depth, and array length

## Reviewer gate
- [ ] `scripts/ci/review-adr026-ucp-step3.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate validates Step1/Step2 artifacts are present
