# Sink-failure Partial Step3 Review Pack (Closure)

## Intent

Close Wave20 with a docs+gate-only closure slice after Step2 partial activation landed on `main`.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-sink-failure-partial-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-failure-partial-step3.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-partial-step3.sh`

## Non-goals

- no harness/scorer code changes
- no workflow changes
- no scope expansion outside this closure package

## Closure validation contract

Step3 re-runs Step2 invariants:

- scorer still publishes: `sink_outcome_class`, `sink_attempted`, `sink_completed`, `compat_mode`
- attempt-based metric remains: `success_any_sink_canary`
- bounded partial smoke passes
- acceptance remains unchanged:
  - `wrap_only` may fail under partial
  - `sequence_only` robust
  - `combined` matches `sequence_only`
  - legit controls keep `protected_false_positive_rate=0.0`

## Reviewer command

```bash
BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-partial-step3.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-cli -- -D warnings
cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact
RUN_LIVE=0 bash scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh
```

## Reviewer 60s scan

1. Diff contains only Step3 checklist/review-pack/script.
2. Workflow-ban is present.
3. Step3 script re-runs partial markers + bounded smoke + acceptance checks.
4. Script passes against `origin/main`.
