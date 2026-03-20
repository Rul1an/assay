# SPLIT REVIEW PACK — Wave O2 Metric Spans Step1

## Intent
Expose per-metric evaluation observability from the core runner while keeping the wave tightly scoped to the runner, one dedicated integration test, and review artifacts.

## Allowed files
- `crates/assay-core/src/engine/runner.rs`
- `crates/assay-core/src/engine/runner_next/execute.rs`
- `crates/assay-core/tests/runner_metric_spans.rs`
- `docs/contributing/SPLIT-INVENTORY-wave-o2-metric-spans-step1.md`
- `docs/contributing/SPLIT-CHECKLIST-wave-o2-metric-spans-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-o2-metric-spans-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-o2-metric-spans-step1.md`
- `scripts/ci/review-wave-o2-metric-spans-step1.sh`

## What reviewers should verify
1. The diff stays inside the allowlist above.
2. The runner still evaluates metrics in the same order and short-circuits the same way on unstable/fail outcomes.
3. `run_suite()` propagates the active subscriber into spawned worker tasks.
4. `assay.eval.metric` spans always capture metric identity and latency.
5. Metric evaluation errors now surface in tracing without swallowing or rewriting the original error.
6. Existing OTel contract tests still pass unchanged.
7. No Wave O1 ringbuf or unrelated supply-chain changes leaked into this step.

## Proof snippets
- Span hook:
  - `info_span!("assay.eval.metric", ...)`
  - `.instrument(metric_span.clone())`
- Subscriber propagation:
  - `.with_current_subscriber()`
- Recorded success fields:
  - `assay.eval.metric.score`
  - `assay.eval.metric.passed`
  - `assay.eval.metric.duration_ms`
- Recorded error fields:
  - `error`
  - `error.message`
- Contract tests:
  - `runner_metric_spans_record_success_fields`
  - `runner_metric_spans_record_error_fields`

## Reviewer command
```bash
BASE_REF=origin/codex/codebase-analysis-observability bash scripts/ci/review-wave-o2-metric-spans-step1.sh
```

## Validation note
The reviewer script stays inside `assay-core`, reruns the full `--lib` suite to catch parallel-test regressions, and reruns the dedicated `runner_metric_spans` plus existing `otel_contract` integration tests to prove the new runner spans did not disturb the tracing contract.
