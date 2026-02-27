# SPLIT CHECKLIST - ADR-026 Step3 (closure)

## Scope
- [ ] Only Step3 closure docs and reviewer gate changed
- [ ] No `.github/workflows/*` changes
- [ ] No runtime behavior changes

## Contracts present
- [ ] `crates/assay-adapter-api/` exists
- [ ] `crates/assay-adapter-acp/` exists
- [ ] ACP fixtures exist under `scripts/ci/fixtures/adr026/acp/v2.11.0/`
- [ ] `scripts/ci/test-adapter-acp.sh` exists and passes

## Step1/Step2 coverage
- [ ] Adapter API contract frozen (`ProtocolAdapter`, `AttachmentWriter`, strict/lenient)
- [ ] ACP MVP implemented
- [ ] Negative fixtures included
- [ ] Determinism asserted on repeated happy-path conversion

## Reviewer gate
- [ ] `scripts/ci/review-adr026-step3.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate validates Step1/Step2 artifacts are present
