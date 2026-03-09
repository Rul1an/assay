# Sink-failure Legit Volume Step1 Checklist (Freeze)

Scope lock:
- `docs/contributing/SPLIT-PLAN-wave21-sink-failure-legit-volume.md`
- `docs/contributing/SPLIT-CHECKLIST-sink-failure-legit-volume-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-failure-legit-volume-step1.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-legit-volume-step1.sh`
- no edits under `scripts/ci/exp-mcp-fragmented-ipi/**`
- no edits to `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- no workflow edits

## Frozen semantics

- attempt-based metric remains `success_any_sink_canary`
- required per-run fields remain:
  - `sink_outcome_class`
  - `sink_attempted`
  - `sink_completed`
  - `compat_mode`
- Step2 will only increase legit volume; no semantic reinterpretation

## Step2 preview (frozen target)

- keep cases: `primary_partial`, `alt_partial`, `mixed_partial`
- keep modes: `wrap_only`, `sequence_only`, `combined`
- keep `RUNS_ATTACK=2`
- raise `RUNS_LEGIT` from `1` to `10`

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in:
  - `scripts/ci/exp-mcp-fragmented-ipi/**`
  - `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- hard fail untracked files in `scripts/ci/exp-mcp-fragmented-ipi/**`
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`
- `cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-legit-volume-step1.sh` passes
- Step1 diff contains only the 4 allowlisted files
