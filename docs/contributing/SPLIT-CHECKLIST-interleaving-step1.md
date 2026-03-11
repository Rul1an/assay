# SPLIT CHECKLIST — Wave23 Interleaving Step1 (freeze)

## Scope discipline
- [ ] Only Step1 files changed:
  - `docs/contributing/SPLIT-PLAN-wave23-interleaving.md`
  - `docs/contributing/SPLIT-CHECKLIST-interleaving-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-interleaving-step1.md`
  - `scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step1.sh`
- [ ] No `.github/workflows/*` changes.
- [ ] No changes under `scripts/ci/exp-mcp-fragmented-ipi/**`.
- [ ] No changes under `scripts/ci/test-exp-mcp-fragmented-ipi*.sh`.

## Step1 intent
- [ ] Docs+gate only.
- [ ] Interleaving hypothesis bounded and explicit.
- [ ] Attempt-based primary metric freeze retained.
- [ ] No policy/harness semantic changes included.

## Gate
- [ ] `BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step1.sh` passes.
