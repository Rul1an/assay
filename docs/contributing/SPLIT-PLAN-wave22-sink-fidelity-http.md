# Wave22 Plan - Sink-failure Fidelity Upgrade (Offline HTTP)

## Goal

Increase sink fidelity in the existing fragmented-IPI sink-failure line by adding a local offline HTTP-egress sink path.

This wave must preserve governance semantics and remain hermetic/deterministic.

## A/B/C slicing

- A: Step1 freeze (docs+gate only)
- B: Step2 bounded implementation (offline HTTP sink fidelity path)
- C: Step3 closure (docs+gate only)

## Step1 (A) - freeze

Branch: `codex/wave22-sink-fidelity-http-step1-freeze` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave22-sink-fidelity-http.md`
- `docs/contributing/SPLIT-CHECKLIST-sink-fidelity-http-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-fidelity-http-step1.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step1.sh`

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

Primary governance metric remains unchanged:
- `success_any_sink_canary` (attempt-based)

Required per-run fields remain published:
- `sink_outcome_class`
- `sink_attempted`
- `sink_completed`
- `compat_mode`

Wave22 adds completion-detail publication but does not reinterpret the primary metric.

## Step2 (B) - bounded fidelity upgrade preview

Bounded scope (preview):
- add a local offline HTTP-egress sink path in experiment harness/compat host
- keep governance policy behavior unchanged
- keep matrix modes unchanged (`wrap_only`, `sequence_only`, `combined`)
- keep run shape unchanged for comparability:
  - `RUNS_ATTACK=2`
  - `RUNS_LEGIT=100`
- keep existing sink-failure case set (`primary_partial`, `alt_partial`, `mixed_partial`)

Frozen fidelity constraints:
- localhost-only
- offline-only
- deterministic fixtures/responses
- no external network dependency

Frozen publication additions (completion layer):
- `egress_http_status_class`
- `payload_delivered`
- `response_observed`

No new matrix axes in this slice.

## Step3 (C) - closure

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
