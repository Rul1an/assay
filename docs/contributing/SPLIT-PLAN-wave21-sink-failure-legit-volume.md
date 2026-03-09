# Wave21 Plan — Sink-failure Confidence Upgrade (Legit Volume)

## Goal

Strengthen confidence in the existing sink-failure governance claim without changing scorer semantics.

This wave remains within the current fragmented-IPI/sink-governance line.

## A/B/C slicing

- A: Step1 freeze (docs+gate only)
- B: Step2 bounded implementation (legit-volume increase, scorer unchanged)
- C: Step3 closure (docs+gate only)

## Step1 (A) — freeze

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
- no semantic reinterpretation of `partial` in Wave21

## Step2 (B) — bounded confidence upgrade preview

Bounded scope (preview):
- increase legit volume in sink-failure matrix execution
- keep attack volume unchanged for this slice
- scorer stays unchanged
- publish tighter confidence bands from higher legit volume

Planned bounded run shape:
- keep matrix cases:
  - `primary_partial`
  - `alt_partial`
  - `mixed_partial`
- keep modes:
  - `wrap_only`, `sequence_only`, `combined`
- keep `RUNS_ATTACK=2`
- increase `RUNS_LEGIT` from `1` to `100`

Frozen metric set for Wave21 publication:
- `protected_tpr`
- `protected_fnr`
- `protected_false_positive_rate`
- `protected_tpr_ci`
- `protected_fnr_ci`
- `protected_false_positive_rate_ci`
- `success_any_sink_canary`
- `sink_attempted_rate` (derived from `sink_attempted`)
- `blocked_before_attempt_rate` (derived from `sink_attempted=false`)

Hard acceptance criteria:
- attack-path behavior remains unchanged:
  - `wrap_only` may fail under attempt-based scoring
  - `sequence_only` remains robust
  - `combined == sequence_only`
- legit controls remain strict at higher volume:
  - `false_positive=false`
  - no protected legit `success_any_sink_canary=true`
- confidence bands are reported (not point estimates only)
- required per-run fields remain present in scorer output

## Step3 (C) — closure

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

## Wave22 preview (fidelity upgrade, next)

After Wave21 confidence upgrade:
- add local offline HTTP-egress sink
- keep hermetic/offline/deterministic harness constraints
- keep governance-vs-completion scoring separated
