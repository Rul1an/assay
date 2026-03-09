# Sink-fidelity HTTP Step1 Review Pack (Freeze)

## Intent

Freeze Wave22 scope for bounded sink-fidelity upgrade in the fragmented-IPI sink-failure line.

Wave22 introduces a local offline HTTP sink path, while preserving governance semantics.

## Scope

- `docs/contributing/SPLIT-PLAN-wave22-sink-fidelity-http.md`
- `docs/contributing/SPLIT-CHECKLIST-sink-fidelity-http-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-fidelity-http-step1.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step1.sh`

## Non-goals

- no changes under `scripts/ci/exp-mcp-fragmented-ipi/**`
- no changes to `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- no workflow changes
- no policy or scorer semantics changes in Step1

## Frozen Wave22 contract

- primary metric remains: `success_any_sink_canary`
- required fields remain: `sink_outcome_class`, `sink_attempted`, `sink_completed`, `compat_mode`
- run shape remains fixed:
  - `RUNS_ATTACK=2`
  - `RUNS_LEGIT=100`
- fidelity constraints are frozen:
  - localhost-only
  - offline-only
  - deterministic
  - no external network dependency
- publication adds completion fields:
  - `egress_http_status_class`
  - `payload_delivered`
  - `response_observed`

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-cli -- -D warnings
cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact
```

## Reviewer 60s scan

1. Confirm diff is only the 4 Step1 files.
2. Confirm workflow-ban and experiment subtree bans are present.
3. Confirm frozen run-shape markers remain fixed (`RUNS_ATTACK=2`, `RUNS_LEGIT=100`).
4. Confirm frozen fidelity constraints are explicit (localhost/offline/deterministic).
5. Run reviewer script and expect PASS.
