# Sink-failure Partial Step1 Review Pack (Freeze)

## Intent

Freeze Wave20 scope for completing the missing `partial` branch in the sink-failure experiment line.

## Scope

- `docs/contributing/SPLIT-PLAN-wave20-sink-failure-partial.md`
- `docs/contributing/SPLIT-CHECKLIST-sink-failure-partial-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-failure-partial-step1.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-partial-step1.sh`

## Non-goals

- no changes under `scripts/ci/exp-mcp-fragmented-ipi/**`
- no workflow changes
- no scoring or harness behavior change in Step1

## Frozen interpretation constraints

- `partial` is sink-attempted and neither clean success nor hard fail
- scoring remains attempt-based (`success_any_sink_canary`)
- Step2 must publish: `sink_outcome_class`, `sink_attempted`, `sink_completed`, `compat_mode`

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-partial-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-cli -- -D warnings
cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact
```

## Reviewer 60s scan

1. Confirm diff is only the 4 Step1 files.
2. Confirm workflow-ban and experiment-subtree bans exist in the script.
3. Confirm frozen `partial` semantics are explicit in plan/checklist.
4. Run reviewer script and expect PASS.
