# Wave21 Plan — Sink-failure Legit Volume Increase

## Goal

Increase legit-run volume in the sink-failure experiment line without changing experiment semantics.

This wave is bounded to the sink-failure experiment harness and reporting surface.

## Step1 (freeze)

Branch: `codex/wave21-sink-failure-legit-volume-step1-freeze` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave21-sink-failure-legit-volume.md`
- `docs/contributing/SPLIT-CHECKLIST-sink-failure-legit-volume-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-failure-legit-volume-step1.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-legit-volume-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `scripts/ci/exp-mcp-fragmented-ipi/**`
- no edits to `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- no workflow edits

Step1 gate:
- allowlist-only diff (the 4 Step1 files)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in:
  - `scripts/ci/exp-mcp-fragmented-ipi/**`
  - `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- hard fail untracked files in `scripts/ci/exp-mcp-fragmented-ipi/**`
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`
- `cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact`

## Frozen semantics (must stay stable)

- scoring remains attempt-based:
  - primary metric: `success_any_sink_canary`
- required per-run fields remain published:
  - `sink_outcome_class`
  - `sink_attempted`
  - `sink_completed`
  - `compat_mode`
- partial/timing interpretation must not be changed by this wave

## Step2 (bounded implementation preview)

Bounded scope (preview):
- increase legit volume in sink-failure matrix execution
- keep attack volume unchanged for this slice
- update docs/results for legit-volume batch

Planned bounded run shape:
- keep matrix cases:
  - `primary_partial`
  - `alt_partial`
  - `mixed_partial`
- keep modes:
  - `wrap_only`, `sequence_only`, `combined`
- keep `RUNS_ATTACK=2`
- increase `RUNS_LEGIT` from `1` to `10`

Hard acceptance criteria:
- attack-path behavior remains unchanged:
  - `wrap_only` may fail under attempt-based scoring
  - `sequence_only` remains robust
  - `combined == sequence_only`
- legit controls remain strict at higher volume:
  - `false_positive=false`
  - no protected legit `success_any_sink_canary=true`
- required per-run fields remain present in scorer output

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
