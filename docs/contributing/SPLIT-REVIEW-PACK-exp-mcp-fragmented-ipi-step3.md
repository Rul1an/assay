# Review Pack - Experiment MCP Fragmented IPI Step3 (closure)

## Intent
Close the fragmented IPI experiment loop by adding closure review artifacts for the Step1 freeze and Step2 harness implementation.

## Scope
- docs/contributing/SPLIT-CHECKLIST-exp-mcp-fragmented-ipi-step3.md
- docs/contributing/SPLIT-REVIEW-PACK-exp-mcp-fragmented-ipi-step3.md
- scripts/ci/review-exp-mcp-fragmented-ipi-step3.sh

## Non-goals
- No workflow changes
- No runtime product changes
- No new policy primitives
- No experiment result interpretation beyond what the existing Step2 scorer emits

## What should already exist
- `docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-2026q1.md`
- `docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-POLICY-CONTRACT.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-step1.sh`
- `scripts/ci/fixtures/exp-mcp-fragmented-ipi/`
- `scripts/ci/exp-mcp-fragmented-ipi/`
- `scripts/ci/test-exp-mcp-fragmented-ipi.sh`
- `scripts/ci/review-exp-mcp-fragmented-ipi-step2.sh`

## Verification
- `BASE_REF=origin/codex/exp-mcp-fragmented-ipi-step2-harness bash scripts/ci/review-exp-mcp-fragmented-ipi-step3.sh`
- `bash scripts/ci/test-exp-mcp-fragmented-ipi.sh`
- `cargo fmt --check`
- `cargo clippy -p assay-cli -p assay-mcp-server -- -D warnings`

## Reviewer 60s scan
1. Confirm Step3 changes are docs + gate only.
2. Confirm no `.github/workflows/*` changes.
3. Run the Step3 reviewer gate.
4. Confirm the checklist matches the actual Step1/Step2 experiment artifacts and the current sequence-sidecar design.
