# Sink-fidelity HTTP Step2 Review Pack (Bounded Implementation)

## Intent

Execute Wave22 fidelity upgrade by adding a local offline HTTP-egress sink path while preserving existing sink-failure governance semantics.

## Allowed scope

- `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- `scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py`
- `scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py`
- `scripts/ci/exp-mcp-fragmented-ipi/score_sink_failure.py`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026Q1-RERUN.md`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-FIDELITY-HTTP-2026Q1-RESULTS.md`
- Step2 docs/gate files only

## Non-goals

- no workflow changes
- no policy behavior changes
- no new matrix axes
- no scoring reinterpretation of `success_any_sink_canary`

## Frozen checks

- run shape fixed:
  - `RUNS_ATTACK=2`
  - `RUNS_LEGIT=100`
- fidelity marker fixed:
  - `SINK_FIDELITY_MODE=http_local`
- completion publication present:
  - `egress_http_status_class`
  - `payload_delivered`
  - `response_observed`

## Acceptance

- `wrap_only` remains inferior where expected
- `sequence_only` and `combined` remain robust on protected attack path
- `combined == sequence_only` on protected outcomes
- protected legit false-positive rate stays `0.0`
- compatibility mode marker is HTTP-local:
  - `sink_failure_compat_host_http_local_v1`

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step2.sh
```
