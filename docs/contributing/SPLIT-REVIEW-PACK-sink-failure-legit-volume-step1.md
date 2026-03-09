# Sink-failure Legit Volume Step1 Review Pack (Freeze)

## Intent

Freeze Wave21 scope for confidence upgrade in the sink-failure experiment line.

Wave21 increases legit-run volume only; it does not change scorer semantics.

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

## Frozen Wave21 contract

- attempt-based metric remains: `success_any_sink_canary`
- required fields remain: `sink_outcome_class`, `sink_attempted`, `sink_completed`, `compat_mode`
- Step2 target is fixed:
  - `RUNS_ATTACK=2`
  - `RUNS_LEGIT=100`
  - modes/cases unchanged
- publication must include confidence bands + derived attempt/pre-attempt rates

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
2. Confirm workflow-ban and subtree bans are present in script.
3. Confirm Step2 target freeze (`RUNS_ATTACK=2`, `RUNS_LEGIT=100`).
4. Confirm scorer semantics stay unchanged in Step1.
5. Run reviewer script and expect PASS.
