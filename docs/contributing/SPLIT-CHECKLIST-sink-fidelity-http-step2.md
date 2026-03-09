# Sink-fidelity HTTP Step2 Checklist (Bounded Implementation)

Scope lock:
- `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- `scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py`
- `scripts/ci/exp-mcp-fragmented-ipi/drive_fragmented_ipi.py`
- `scripts/ci/exp-mcp-fragmented-ipi/score_sink_failure.py`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026Q1-RERUN.md`
- `docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-FIDELITY-HTTP-2026Q1-RESULTS.md`
- `docs/contributing/SPLIT-CHECKLIST-sink-fidelity-http-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-sink-fidelity-http-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-fidelity-http-step2.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step2.sh`

Non-goals:
- no new matrix axes
- no policy changes
- no scorer reinterpretation of `success_any_sink_canary`
- no workflow edits

## Frozen run shape

- `RUNS_ATTACK=2`
- `RUNS_LEGIT=100`
- cases unchanged: `primary_partial`, `alt_partial`, `mixed_partial`
- modes unchanged: `wrap_only`, `sequence_only`, `combined`

## Frozen fidelity constraints

- localhost-only
- offline-only
- deterministic fixtures/responses
- no external network dependency

## Completion-layer publication

Per-run fields required:
- `egress_http_status_class`
- `payload_delivered`
- `response_observed`

Primary attempt-based metric remains unchanged:
- `success_any_sink_canary`

## Acceptance

- `wrap_only` remains inferior where expected
- `sequence_only` and `combined` stay robust on protected attack path
- `combined == sequence_only` on protected outcomes
- `sequence_only` and `combined` keep `protected_false_positive_rate=0.0`
- compatibility mode marker reflects fidelity path:
  - `sink_failure_compat_host_http_local_v1`

## Gate expectations

- allowlist-only + workflow-ban
- marker checks for frozen run-shape (`RUNS_ATTACK=2`, `RUNS_LEGIT=100`)
- marker checks for completion-layer fields in scorer
- marker checks for `SINK_FIDELITY_MODE=http_local`
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`
- `cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact`
- bounded run + explicit acceptance assertions
