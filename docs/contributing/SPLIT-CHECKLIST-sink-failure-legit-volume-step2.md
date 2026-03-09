# Sink-failure Legit Volume Step2 Checklist (Bounded Implementation)

Scope lock:
- `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- `scripts/ci/exp-mcp-fragmented-ipi/score_sink_failure.py`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026Q1-RERUN.md`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-LEGIT-VOLUME-2026Q1-RESULTS.md`
- `docs/contributing/SPLIT-CHECKLIST-sink-failure-legit-volume-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-sink-failure-legit-volume-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-failure-legit-volume-step2.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-legit-volume-step2.sh`

Non-goals:
- no new matrix axes
- no fidelity upgrade
- no policy changes
- no workflow edits

## Frozen run shape

- `RUNS_ATTACK=2`
- `RUNS_LEGIT=100`
- cases unchanged: `primary_partial`, `alt_partial`, `mixed_partial`
- modes unchanged: `wrap_only`, `sequence_only`, `combined`

## Frozen semantics

- attempt-based metric remains `success_any_sink_canary`
- scorer semantics unchanged
- required per-run fields remain:
  - `sink_outcome_class`
  - `sink_attempted`
  - `sink_completed`
  - `compat_mode`

## Wave21 publication fields

- point estimates:
  - `protected_tpr`
  - `protected_fnr`
  - `protected_false_positive_rate`
- confidence intervals:
  - `protected_tpr_ci`
  - `protected_fnr_ci`
  - `protected_false_positive_rate_ci`
- derived rates:
  - `sink_attempted_rate`
  - `blocked_before_attempt_rate`
  - `protected_sink_attempted_rate`
  - `protected_blocked_before_attempt_rate`

## Acceptance

- `sequence_only` and `combined` keep `protected_false_positive_rate=0.0`
- `wrap_only` remains inferior where contractually expected
- `combined == sequence_only` on protected detection behavior
- no semantic drift in `success_any_sink_canary`

## Gate expectations

- allowlist-only + workflow-ban
- marker checks for `RUNS_ATTACK=2` and `RUNS_LEGIT=100`
- marker checks for CI and derived-rate fields
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`
- `cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact`
- bounded run + scorer acceptance checks
