# SPLIT MOVE MAP — Wave21 Sink-failure Legit Volume Step2

## Intent

Bounded implementation for confidence upgrade only.

No new experiment family and no scorer semantic changes.

## File-level mapping

- `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
  - freeze run shape markers:
    - `RUNS_ATTACK=2`
    - `RUNS_LEGIT=100`
  - keep same cases and modes
  - add assertions for CI/derived publication fields
  - publish aggregate artifact:
    - `sink-failure-legit-volume-summary.json`
  - preserve legacy aggregate artifact for compatibility:
    - `sink-failure-partial-summary.json`

- `scripts/ci/exp-mcp-fragmented-ipi/score_sink_failure.py`
  - keep `success_any_sink_canary` semantics unchanged
  - add derived totals/rates in `conditions.*`:
    - `blocked_before_attempt_total`
    - `sink_attempted_rate`
    - `blocked_before_attempt_rate`
  - add top-level derived rates:
    - `sink_attempted_rate`
    - `blocked_before_attempt_rate`
    - `baseline_sink_attempted_rate`
    - `baseline_blocked_before_attempt_rate`
    - `protected_sink_attempted_rate`
    - `protected_blocked_before_attempt_rate`
  - keep point estimates and CI fields unchanged

- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026Q1-RERUN.md`
  - document Wave21 bounded confidence run shape
  - include Wave21 aggregate artifact path

- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-LEGIT-VOLUME-2026Q1-RESULTS.md`
  - publish bounded Wave21 legit-volume run outputs
  - include acceptance interpretation and limitations

## Explicit non-moves

- no edits to compat-host logic
- no edits to policy files
- no edits to workflow files
- no new matrix case identifiers
