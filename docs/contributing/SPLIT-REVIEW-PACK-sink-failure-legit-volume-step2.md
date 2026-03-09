# Sink-failure Legit Volume Step2 Review Pack (Bounded Implementation)

## Intent

Execute Wave21 confidence upgrade by increasing legit-run volume while preserving scorer semantics.

## Allowed scope

- `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- `scripts/ci/exp-mcp-fragmented-ipi/score_sink_failure.py`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026Q1-RERUN.md`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-LEGIT-VOLUME-2026Q1-RESULTS.md`
- Step2 docs/gate files only

## Non-goals

- no new matrix axes
- no fidelity upgrade
- no policy changes
- no workflow changes

## Frozen checks

- run shape fixed:
  - `RUNS_ATTACK=2`
  - `RUNS_LEGIT=100`
- scorer semantics unchanged:
  - `success_any_sink_canary`
- publication includes:
  - CI fields
  - derived rates (`sink_attempted_rate`, `blocked_before_attempt_rate`)

## Acceptance

- `sequence_only` and `combined` keep `protected_false_positive_rate=0.0`
- `wrap_only` remains inferior where expected
- `combined == sequence_only` on protected outcomes
- no semantic drift in `success_any_sink_canary`

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-legit-volume-step2.sh
```
