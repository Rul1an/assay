# SPLIT CHECKLIST — MCP Fragmented-IPI Line Closure (docs-only)

## Scope discipline
- [ ] Diff is restricted to:
  - `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md`
  - `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-FIDELITY-HTTP-2026Q1-RESULTS.md`
  - `docs/contributing/SPLIT-CHECKLIST-exp-mcp-fragmented-ipi-line-closure.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-exp-mcp-fragmented-ipi-line-closure.md`
  - `scripts/ci/review-exp-mcp-fragmented-ipi-line-closure.sh`
- [ ] No `.github/workflows/*` changes.
- [ ] No harness/scorer/runtime code changes.

## Closure content checks
- [ ] Main results doc contains one final line table covering:
  - main fragmented-IPI
  - wrap-bypass
  - second-sink
  - cross-session decay
  - sink-failure
  - sink-failure partial
  - sink-fidelity HTTP
  - interleaving (mixed legit+malicious)
- [ ] Main results doc includes bounded core claim:
  - `sequence_only` decisive
  - `combined` follows `sequence_only`
  - `wrap_only` insufficient
- [ ] Main results doc includes explicit limits:
  - attempt-based metric
  - offline/local sink boundary
  - bounded matrix + CI reporting boundary
- [ ] Fidelity-HTTP results doc includes `DEC-007 closure note` with:
  - proven scope
  - not-proven scope

## Reviewer command
- [ ] `BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-line-closure.sh` passes.
