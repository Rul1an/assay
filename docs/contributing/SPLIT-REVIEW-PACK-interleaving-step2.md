# SPLIT REVIEW PACK — Wave23 Interleaving Step2

## Intent
Implement the bounded interleaving branch for the fragmented-IPI experiment family.

Step2 scope is limited to:
- harness activation for `EXPERIMENT_VARIANT=interleaving`
- new interleaving scorer and runner
- rerun/results documentation
- a strict reviewer gate

No workflow or unrelated experiment drift is allowed.

## Allowed files
- `scripts/ci/test-exp-mcp-fragmented-ipi-interleaving.sh`
- `scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py`
- `scripts/ci/exp-mcp-fragmented-ipi/mock_mcp_server.py`
- `scripts/ci/exp-mcp-fragmented-ipi/score_interleaving.py`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-INTERLEAVING-2026Q1-RERUN.md`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-INTERLEAVING-2026Q1-RESULTS.md`
- `docs/contributing/SPLIT-CHECKLIST-interleaving-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-interleaving-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-interleaving-step2.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step2.sh`

## Reviewer checks
1. Diff is allowlist-only and no `.github/workflows/*` file is touched.
2. Driver keeps the primary metric attempt-based and emits interleaving markers.
3. New scorer publishes CI + derived rates.
4. Bounded runner passes acceptance:
   - wrap-only remains the expected weak baseline
   - sequence-only and combined are robust on protected attack path
   - combined matches sequence-only
   - legit controls remain strict

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step2.sh
```
