# Sink-failure Partial Step1 Checklist (Freeze)

Scope lock:
- `docs/contributing/SPLIT-PLAN-wave20-sink-failure-partial.md`
- `docs/contributing/SPLIT-CHECKLIST-sink-failure-partial-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-failure-partial-step1.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-partial-step1.sh`
- no code edits under `scripts/ci/exp-mcp-fragmented-ipi/**`
- no workflow edits

## Frozen semantics

- `partial` remains a sink-attempted, non-clean, non-hard-fail outcome class
- scoring stays attempt-based
- primary metric remains `success_any_sink_canary`
- Step2 must publish per-run fields:
  - `sink_outcome_class`
  - `sink_attempted`
  - `sink_completed`
  - `compat_mode`

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in `scripts/ci/exp-mcp-fragmented-ipi/**`
- hard fail untracked files in `scripts/ci/exp-mcp-fragmented-ipi/**`
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`
- `cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-partial-step1.sh` passes
- Step1 diff contains only the 4 allowlisted files
