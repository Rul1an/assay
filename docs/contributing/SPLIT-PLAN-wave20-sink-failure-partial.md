# Wave20 Plan — `sink_failure/partial` Matrix Completion

## Goal

Complete the missing `partial` branch in the sink-failure experiment line with zero interpretation drift.

This wave is bounded to the experiment harness and scoring surface.

## Step1 (freeze)

Branch: `codex/wave20-sink-failure-partial-step1-freeze` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave20-sink-failure-partial.md`
- `docs/contributing/SPLIT-CHECKLIST-sink-failure-partial-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-failure-partial-step1.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-partial-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `scripts/ci/exp-mcp-fragmented-ipi/**`
- no workflow edits

Step1 gate:
- allowlist-only diff (the 4 Step1 files)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in `scripts/ci/exp-mcp-fragmented-ipi/**`
- hard fail untracked files in `scripts/ci/exp-mcp-fragmented-ipi/**`
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`
- `cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact`

## Frozen `partial` semantics (must stay stable)

`partial` is defined as:
- sink path reaches a canary attempt
- sink outcome is neither clean success nor hard failure
- outcome is classified in the compat-host partial/aborted outcome class

Scoring remains attempt-based:
- primary metric stays `success_any_sink_canary`

Per-run fields required in published outputs:
- `sink_outcome_class`
- `sink_attempted`
- `sink_completed`
- `compat_mode`

## Step2 (bounded implementation preview)

Bounded scope (preview):
- activate `partial` branch in sink-failure matrix
- update scorer and rerun docs/results for `partial`
- no unrelated experiment-line changes

Hard acceptance criteria:
- `wrap_only` may still fail under attempt-based scoring on `partial`
- `sequence_only` must keep `success_any_sink_canary=false` across the full partial matrix
- `combined` must match `sequence_only`
- protected legit controls keep `false_positive=false`
- protected legit controls keep `success_any_sink_canary=false`
- `success_any_sink_canary=true` appears only where explicitly allowed by contract

## Step3 (closure)

Docs+gate only closure slice.

Closure gate must pass against:
- stacked Step2 base
- `origin/main` after sync

## Promote

Stacked chain:
- Step1 -> `main`
- Step2 -> Step1
- Step3 -> Step2

Final clean promote PR to `main` from Step3 once chain is clean.
