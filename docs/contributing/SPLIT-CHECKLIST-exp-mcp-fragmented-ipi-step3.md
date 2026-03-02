# SPLIT CHECKLIST - Experiment MCP Fragmented IPI Step3 (closure)

## Scope
- [ ] Only Step3 closure docs and reviewer gate changed
- [ ] No `.github/workflows/*` changes
- [ ] No runtime product behavior changes

## Step1 freeze present
- [ ] `docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-2026q1.md` exists
- [ ] `docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-POLICY-CONTRACT.md` exists
- [ ] Step1 reviewer gate exists

## Step2 harness present
- [ ] Fragmented IPI fixtures exist under `scripts/ci/fixtures/exp-mcp-fragmented-ipi/`
- [ ] Baseline and protected policy fixtures exist
- [ ] `scripts/ci/test-exp-mcp-fragmented-ipi.sh` exists and passes
- [ ] `scripts/ci/exp-mcp-fragmented-ipi/score_runs.py` emits deterministic canary metrics
- [ ] Protected mode uses wrap policy plus `assay_check_sequence` sidecar

## Reviewer gate
- [ ] `scripts/ci/review-exp-mcp-fragmented-ipi-step3.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate validates Step1/Step2 artifacts are present
