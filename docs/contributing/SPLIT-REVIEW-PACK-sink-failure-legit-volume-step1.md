# Sink-failure Legit Volume Step1 Review Pack (Freeze)

## Intent

Freeze Wave21 scope for increasing legit-run volume in the sink-failure experiment line.

## Scope

- `docs/contributing/SPLIT-PLAN-wave21-sink-failure-legit-volume.md`
- `docs/contributing/SPLIT-CHECKLIST-sink-failure-legit-volume-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-failure-legit-volume-step1.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-legit-volume-step1.sh`

## Non-goals

- no changes under `scripts/ci/exp-mcp-fragmented-ipi/**`
- no changes to `scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh`
- no workflow changes
- no scoring semantics changes in Step1

## Frozen constraints

- attempt-based metric remains: `success_any_sink_canary`
- required fields remain: `sink_outcome_class`, `sink_attempted`, `sink_completed`, `compat_mode`
- Step2 only increases legit-run volume (`RUNS_LEGIT 1 -> 10`) while keeping attack volume stable (`RUNS_ATTACK=2`)

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-legit-volume-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-cli -- -D warnings
cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact
```

## Reviewer 60s scan

1. Confirm diff is only the 4 Step1 files.
2. Confirm workflow-ban and experiment-subtree bans exist in script.
3. Confirm legit-volume target is frozen (`RUNS_ATTACK=2`, `RUNS_LEGIT=10`).
4. Run reviewer script and expect PASS.
