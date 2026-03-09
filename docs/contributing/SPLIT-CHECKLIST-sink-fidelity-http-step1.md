# Sink-fidelity HTTP Step1 Checklist (Freeze)

Scope lock:
- `docs/contributing/SPLIT-PLAN-wave22-sink-fidelity-http.md`
- `docs/contributing/SPLIT-CHECKLIST-sink-fidelity-http-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-fidelity-http-step1.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step1.sh`
- no edits under `scripts/ci/exp-mcp-fragmented-ipi/**`
- no edits to `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- no workflow edits

## Frozen semantics

- primary metric remains attempt-based: `success_any_sink_canary`
- required fields remain:
  - `sink_outcome_class`
  - `sink_attempted`
  - `sink_completed`
  - `compat_mode`
- no policy changes in Wave22 Step1

## Frozen Wave22 Step2 target

- keep cases: `primary_partial`, `alt_partial`, `mixed_partial`
- keep modes: `wrap_only`, `sequence_only`, `combined`
- keep `RUNS_ATTACK=2`
- keep `RUNS_LEGIT=100`
- add bounded fidelity path only:
  - localhost-only
  - offline-only
  - deterministic
  - no external network dependency

## Frozen publication additions

- `egress_http_status_class`
- `payload_delivered`
- `response_observed`

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

- `BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step1.sh` passes
- Step1 diff contains only the 4 allowlisted files
