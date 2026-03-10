# SPLIT REVIEW PACK — MCP Fragmented-IPI Line Closure

## Intent
Close the fragmented-IPI experiment family with a docs-only closure slice.

This slice finalizes:
- one line-wide summary table in the main results doc
- one explicit DEC-007 closure note in the Wave22 fidelity results doc
- one reviewer gate to keep scope and claims bounded

## Allowed files
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-FIDELITY-HTTP-2026Q1-RESULTS.md`
- `docs/contributing/SPLIT-CHECKLIST-exp-mcp-fragmented-ipi-line-closure.md`
- `docs/contributing/SPLIT-REVIEW-PACK-exp-mcp-fragmented-ipi-line-closure.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-line-closure.sh`

## Not allowed
- `.github/workflows/*`
- any `scripts/ci/exp-mcp-fragmented-ipi/*` harness changes
- any scorer/runtime/policy code changes

## Reviewer checks
1. Diff scope is docs + reviewer script only.
2. Main results doc has final line table + bounded claim + explicit limits.
3. Fidelity-HTTP results doc has DEC-007 closure note with proven and not-proven boundaries.
4. No workflow or experiment-runtime drift.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-line-closure.sh
```

Expected result:
- gate passes
- closure claims stay bounded
- experiment line is ready to mark as closed-loop in companion logs
