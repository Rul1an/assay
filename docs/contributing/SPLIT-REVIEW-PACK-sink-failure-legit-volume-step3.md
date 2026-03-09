# Sink-failure Legit Volume Step3 Review Pack (Closure)

## Intent

Close Wave21 with a docs+gate-only closure slice after Step2 legit-volume implementation landed on `main`.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-sink-failure-legit-volume-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-sink-failure-legit-volume-step3.md`
- `scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-legit-volume-step3.sh`

## Non-goals

- no harness/scorer code changes
- no workflow changes
- no scope expansion outside this closure package

## Closure validation contract

Step3 re-runs Step2 invariants:

- frozen run shape remains: `RUNS_ATTACK=2`, `RUNS_LEGIT=100`
- attempt-based metric remains: `success_any_sink_canary`
- scorer output still includes:
  - CI fields
  - derived rates (`sink_attempted_rate`, `blocked_before_attempt_rate`)
- bounded legit-volume smoke passes
- acceptance remains unchanged:
  - `sequence_only` and `combined` keep `protected_false_positive_rate=0.0`
  - `wrap_only` remains inferior where expected
  - `combined` equals `sequence_only` on protected outcomes

## Reviewer command

```bash
BASE_REF=origin/main bash scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-legit-volume-step3.sh
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
3. Step3 script re-runs run-shape markers + CI/derived markers + bounded smoke + acceptance checks.
4. Script passes against `origin/main`.
