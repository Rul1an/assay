# Sink-failure Legit Volume Step3 Checklist (Closure)

Scope lock:
- `docs/contributing/SPLIT-CHECKLIST-sink-failure-legit-volume-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-failure-legit-volume-step3.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-legit-volume-step3.sh`
- docs+gate only; no code changes in Step3
- no workflow edits

## Closure invariants (re-run)

- frozen run shape remains:
  - `RUNS_ATTACK=2`
  - `RUNS_LEGIT=100`
- scorer semantics unchanged:
  - attempt-based metric remains `success_any_sink_canary`
- scorer output remains published:
  - CI fields
  - derived rates (`sink_attempted_rate`, `blocked_before_attempt_rate`)
- bounded legit-volume smoke remains green
- acceptance remains:
  - `sequence_only` and `combined` keep `protected_false_positive_rate=0.0`
  - `wrap_only` remains inferior where expected
  - `combined` equals `sequence_only` on protected outcomes

## Gate expectations

- allowlist-only diff vs `BASE_REF`
- workflow-ban (`.github/workflows/*`)
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`
- `cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact`
- bounded run + explicit acceptance checks

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-legit-volume-step3.sh` passes
- Step3 diff contains only the 3 Step3 allowlisted files
