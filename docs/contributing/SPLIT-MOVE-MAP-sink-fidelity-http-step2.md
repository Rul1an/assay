# SPLIT MOVE MAP — Wave22 Sink-fidelity HTTP Step2

## Intent

Add a bounded local offline HTTP-egress sink fidelity layer to the existing sink-failure experiment line.

Primary governance metric remains unchanged:
- `success_any_sink_canary` (attempt-based)

No new matrix axes are introduced.

## File-level mapping

- `scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py`
  - add localhost-only HTTP sink handler (`http_local` mode)
  - preserve existing stdio sink behavior as fallback
  - publish deterministic completion metadata:
    - `egress_http_status_class`
    - `payload_delivered`
    - `response_observed`
    - `compat_mode`

- `scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py`
  - pass through completion metadata from tool call responses into per-call `sink_calls`
  - preserve sink-failure routing and block semantics
  - persist run-level `sink_compat_mode` marker

- `scripts/ci/exp-mcp-fragmented-ipi/score_sink_failure.py`
  - keep attempt-based scoring semantics unchanged
  - add completion-layer per-run fields in `sink_failure` payload:
    - `egress_http_status_class`
    - `payload_delivered`
    - `response_observed`
  - publish aggregate completion distributions/rates

- `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
  - freeze `SINK_FIDELITY_MODE=http_local`
  - keep existing partial matrix and run shape
  - add bounded assertions for completion-layer publication
  - publish aggregate artifact:
    - `sink-failure-fidelity-http-summary.json`

- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026Q1-RERUN.md`
  - document Wave22 bounded fidelity branch and required env marker

- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-FIDELITY-HTTP-2026Q1-RESULTS.md`
  - publish bounded run outputs and acceptance interpretation

## Explicit non-moves

- no workflow edits
- no policy file changes
- no new scenario IDs
- no matrix expansion beyond existing partial cases
- no changes to primary success metric semantics
