# Sink-failure Partial Step3 Checklist (Closure)

Scope lock:
- `docs/contributing/SPLIT-CHECKLIST-sink-failure-partial-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-failure-partial-step3.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-partial-step3.sh`
- docs+gate only; no code changes in Step3
- no workflow edits

## Closure invariants (re-run)

- marker fields remain published in scorer:
  - `sink_outcome_class`
  - `sink_attempted`
  - `sink_completed`
  - `compat_mode`
- attempt-based metric remains `success_any_sink_canary`
- bounded partial smoke remains green
- acceptance remains:
  - `wrap_only` may fail under partial (attempt-based)
  - `sequence_only` robust (`protected_tpr=1.0`, `protected_fnr=0.0`)
  - `combined` equals `sequence_only`
  - legit controls strict (`protected_false_positive_rate=0.0`)

## Gate expectations

- allowlist-only diff vs `BASE_REF`
- workflow-ban (`.github/workflows/*`)
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`
- `cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact`
- bounded smoke + explicit acceptance checks

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-partial-step3.sh` passes
- Step3 diff contains only the 3 Step3 allowlisted files
