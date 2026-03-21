# SPLIT INVENTORY — Wave O2 Metric Spans Step1

## Stack context
- Base branch: `codex/codebase-analysis-observability`
- Step branch: `codex/codebase-analysis-metric-spans`
- Intent: emit per-metric evaluation spans from the core runner so operators can see metric latency and verdicts without widening the current observability wave

## Scope lock
- In scope:
  - metric-evaluation span instrumentation in `assay-core` runner
  - subscriber propagation from `run_suite` into worker tasks so the new spans are observable in real async runs
  - dedicated integration tests that prove success/error span fields are exported
  - wave review artifacts and a scope-gated reviewer script
- Out of scope:
  - ring-buffer telemetry follow-ups already covered by Wave O1
  - policy-rule spans beyond the existing per-metric loop
  - LLM client span shape changes
  - registry trust-root work, Python SDK, fuzzing, SBOM, or workflow changes

## Touched implementation files
- `crates/assay-core/src/engine/runner.rs`
- `crates/assay-core/src/engine/runner_next/execute.rs`
- `crates/assay-core/tests/runner_metric_spans.rs`

## Public surface inventory
- No new public Rust API
- No config schema changes
- New internal tracing span name: `assay.eval.metric`
- New span fields:
  - `assay.eval.test_id`
  - `assay.eval.metric.name`
  - `assay.eval.response.cached`
  - `assay.eval.metric.score`
  - `assay.eval.metric.passed`
  - `assay.eval.metric.unstable`
  - `assay.eval.metric.duration_ms`
  - `error`
  - `error.message`

## LOC baseline vs current

| File | Base LOC | Current LOC | Delta |
|---|---:|---:|---:|
| `crates/assay-core/src/engine/runner.rs` | 661 | 696 | +35 |
| `crates/assay-core/src/engine/runner_next/execute.rs` | 227 | 231 | +4 |
| `crates/assay-core/tests/runner_metric_spans.rs` | 0 | 269 | +269 |

## Validation target
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo check -p assay-core`
- `cargo test -p assay-core --lib`
- `cargo test -p assay-core --test runner_metric_spans`
- `cargo test -p assay-core --test otel_contract`
- `git diff --check`
