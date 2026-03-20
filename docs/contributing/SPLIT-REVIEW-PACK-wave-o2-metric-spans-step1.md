# SPLIT REVIEW PACK — Wave O2 Metric Spans Step1

## Intent
Expose per-metric evaluation observability from the core runner while keeping the wave tightly scoped to a single implementation file plus review artifacts.

## Allowed files
- `crates/assay-core/src/engine/runner.rs`
- `docs/contributing/SPLIT-INVENTORY-wave-o2-metric-spans-step1.md`
- `docs/contributing/SPLIT-CHECKLIST-wave-o2-metric-spans-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-o2-metric-spans-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-o2-metric-spans-step1.md`
- `scripts/ci/review-wave-o2-metric-spans-step1.sh`

## What reviewers should verify
1. The diff stays inside the allowlist above.
2. The runner still evaluates metrics in the same order and short-circuits the same way on unstable/fail outcomes.
3. `assay.eval.metric` spans always capture metric identity and latency.
4. Metric evaluation errors now surface in tracing without swallowing or rewriting the original error.
5. Existing OTel contract tests still pass unchanged.
6. No Wave O1 ringbuf or unrelated supply-chain changes leaked into this step.

## Proof snippets
- Span hook:
  - `info_span!("assay.eval.metric", ...)`
  - `.instrument(metric_span.clone())`
- Recorded success fields:
  - `assay.eval.metric.score`
  - `assay.eval.metric.passed`
  - `assay.eval.metric.duration_ms`
- Recorded error fields:
  - `error`
  - `error.message`
- Contract tests:
  - `runner_contract_metric_span_records_success_fields`
  - `runner_contract_metric_span_records_error_fields`

## Reviewer command
```bash
BASE_REF=origin/codex/codebase-analysis-observability bash scripts/ci/review-wave-o2-metric-spans-step1.sh
```

## Validation note
The reviewer script stays inside `assay-core` and reruns the existing `otel_contract` integration test to prove the new runner spans did not disturb the current LLM tracing contract.
